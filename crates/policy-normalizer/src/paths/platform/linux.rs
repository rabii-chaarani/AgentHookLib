//! Linux filesystem naming metadata, read through directory descriptors.

use std::{
    fs::{File, OpenOptions},
    mem::MaybeUninit,
    os::fd::{AsRawFd, RawFd},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

use super::super::{NamingSemantics, NormalizationError, error::io_error};

const EXT_SUPER_MAGIC: u64 = 0xef53;
const XFS_SUPER_MAGIC: u64 = 0x5846_5342;
const BTRFS_SUPER_MAGIC: u64 = 0x9123_683e;
const TMPFS_MAGIC: u64 = 0x0102_1994;

// ext4 marks a casefold-enabled directory with EXT4_CASEFOLD_FL.  ext2, ext3,
// and ext4 share the same statfs magic, so read the per-directory flag for the
// whole ext family and treat the bit as authoritative when it is present.
const EXT4_CASEFOLD_FL: u32 = 0x4000_0000;

pub(in crate::paths) fn naming(dir: &Path) -> Result<NamingSemantics, NormalizationError> {
    let directory = open_directory(dir)?;
    let fs_type = filesystem_type(directory.as_raw_fd())?;
    semantics_for_magic(fs_type, || ext4_directory_flags(directory.as_raw_fd()))
}

fn open_directory(dir: &Path) -> Result<File, NormalizationError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(dir)
        .map_err(io_error)
}

#[allow(unsafe_code)]
fn filesystem_type(fd: RawFd) -> Result<u64, NormalizationError> {
    let mut stat = MaybeUninit::<libc::statfs>::uninit();

    // SAFETY: `fd` is held open by the caller and `stat` points to writable
    // storage of the ABI-defined `statfs` size. A successful call initializes it.
    let result = unsafe { libc::fstatfs(fd, stat.as_mut_ptr()) };
    if result != 0 {
        return Err(NormalizationError::Inaccessible);
    }

    // SAFETY: `fstatfs` returned success and therefore initialized `stat`.
    // Filesystem magic values are 32-bit. Reinterpret those bits so magics
    // with the top bit set (such as Btrfs) also work on 32-bit Linux ABIs.
    Ok(filesystem_magic(
        unsafe { stat.assume_init() }.f_type as u64,
    ))
}

fn filesystem_magic(f_type: u64) -> u64 {
    f_type as u32 as u64
}

#[allow(unsafe_code)]
fn ext4_directory_flags(fd: RawFd) -> Result<u32, NormalizationError> {
    let mut flags: u32 = 0;

    // SAFETY: `fd` is an open directory and FS_IOC_GETFLAGS writes a 32-bit
    // flags value to the valid writable pointer supplied here. Linux encodes
    // the request with `sizeof(long)`, but ioctl_getflags() stores an `unsigned
    // int`; using u32 keeps the value correct on big-endian 64-bit ABIs too.
    // The ioctl only reads metadata.
    let result = unsafe { libc::ioctl(fd, libc::FS_IOC_GETFLAGS, &mut flags) };
    if result < 0 {
        return Err(NormalizationError::Inaccessible);
    }
    Ok(flags)
}

fn semantics_for_magic<F>(fs_type: u64, ext_flags: F) -> Result<NamingSemantics, NormalizationError>
where
    F: FnOnce() -> Result<u32, NormalizationError>,
{
    match fs_type {
        EXT_SUPER_MAGIC => ext_flags().map(|flags| {
            if flags & EXT4_CASEFOLD_FL != 0 {
                NamingSemantics::AsciiInsensitive
            } else {
                NamingSemantics::Exact
            }
        }),
        XFS_SUPER_MAGIC | BTRFS_SUPER_MAGIC | TMPFS_MAGIC => Ok(NamingSemantics::Exact),
        _ => Err(NormalizationError::UnsupportedNamingSemantics),
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs, path::Path};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn ext_casefold_flag_controls_ascii_case_semantics() {
        assert_eq!(
            semantics_for_magic(EXT_SUPER_MAGIC, || Ok(EXT4_CASEFOLD_FL)),
            Ok(NamingSemantics::AsciiInsensitive),
        );
        assert_eq!(
            semantics_for_magic(EXT_SUPER_MAGIC, || Ok(0)),
            Ok(NamingSemantics::Exact),
        );
    }

    #[test]
    fn supported_non_ext_filesystems_are_exact_without_an_ioctl() {
        for fs_type in [XFS_SUPER_MAGIC, BTRFS_SUPER_MAGIC, TMPFS_MAGIC] {
            assert_eq!(
                semantics_for_magic(fs_type, || panic!("unexpected ext ioctl")),
                Ok(NamingSemantics::Exact),
            );
        }
    }

    #[test]
    fn filesystem_magic_preserves_high_bit_values_on_narrow_long_abis() {
        let btrfs_magic = BTRFS_SUPER_MAGIC as u32 as i32 as i64 as u64;
        assert_eq!(filesystem_magic(btrfs_magic), BTRFS_SUPER_MAGIC,);
        assert_eq!(
            semantics_for_magic(filesystem_magic(btrfs_magic), || panic!(
                "unexpected ext ioctl"
            )),
            Ok(NamingSemantics::Exact),
        );
    }

    #[test]
    fn unknown_filesystems_are_rejected_without_an_ioctl() {
        let ioctl_called = Cell::new(false);
        for fs_type in [0, 0x794c_7630, 0x6573_5546, 0x6969] {
            assert_eq!(
                semantics_for_magic(fs_type, || {
                    ioctl_called.set(true);
                    Ok(0)
                }),
                Err(NormalizationError::UnsupportedNamingSemantics),
            );
        }
        assert!(!ioctl_called.get());
    }

    #[test]
    fn ext_flag_read_errors_are_preserved() {
        assert_eq!(
            semantics_for_magic(EXT_SUPER_MAGIC, || Err(NormalizationError::Inaccessible)),
            Err(NormalizationError::Inaccessible),
        );
    }

    #[test]
    fn native_backend_inspects_directories_without_modifying_them() {
        let dir = tempdir().expect("temporary directory");
        fs::write(dir.path().join("sentinel"), b"unchanged").expect("sentinel");
        let before = directory_names(dir.path());

        let result = naming(dir.path());
        assert!(matches!(
            result,
            Ok(NamingSemantics::Exact | NamingSemantics::AsciiInsensitive)
                | Err(NormalizationError::UnsupportedNamingSemantics
                    | NormalizationError::Inaccessible)
        ));
        assert_eq!(directory_names(dir.path()), before);
        assert_eq!(fs::read(dir.path().join("sentinel")).unwrap(), b"unchanged");
    }

    #[test]
    fn native_backend_rejects_procfs_when_available() {
        let proc = Path::new("/proc");
        if proc.is_dir() {
            assert_eq!(
                naming(proc),
                Err(NormalizationError::UnsupportedNamingSemantics),
            );
        }
    }

    #[test]
    fn native_backend_reports_missing_directories() {
        let dir = tempdir().expect("temporary directory");
        let missing = dir.path().join("missing");
        assert_eq!(naming(&missing), Err(NormalizationError::MissingTarget));
    }

    fn directory_names(dir: &Path) -> Vec<std::ffi::OsString> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .expect("read directory")
            .map(|entry| entry.expect("directory entry").file_name())
            .collect();
        names.sort();
        names
    }
}
