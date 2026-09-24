use std::{
    ffi::{CStr, CString},
    mem::{MaybeUninit, size_of, size_of_val},
    os::unix::ffi::OsStrExt,
    path::Path,
};

use super::super::{NamingSemantics, NormalizationError};

const CASE_SENSITIVE: u32 = libc::VOL_CAP_FMT_CASE_SENSITIVE;
const CASE_PRESERVING: u32 = libc::VOL_CAP_FMT_CASE_PRESERVING;

fn is_local_mount(flags: u32) -> bool {
    flags & (libc::MNT_LOCAL as u32) != 0
}

/// Read the naming behavior of an existing directory's local APFS or HFS+ volume.
///
/// Darwin's volume capabilities report case sensitivity, while `pathconf` reports
/// the same property for the path. Requiring both observations to agree avoids
/// treating missing or contradictory metadata as a host default. APFS and HFS+
/// can equate distinct Unicode spellings, so even case-sensitive modes only
/// establish ASCII-sensitive comparison for this normalizer.
pub(in crate::paths) fn naming(dir: &Path) -> Result<NamingSemantics, NormalizationError> {
    let path = CString::new(dir.as_os_str().as_bytes())
        .map_err(|_| NormalizationError::UnsupportedNamingSemantics)?;

    let filesystem =
        native::local_filesystem(&path).ok_or(NormalizationError::UnsupportedNamingSemantics)?;
    let pathconf_case =
        native::case_sensitivity(&path).ok_or(NormalizationError::UnsupportedNamingSemantics)?;
    let (capabilities, valid) =
        native::volume_capabilities(&path).ok_or(NormalizationError::UnsupportedNamingSemantics)?;

    classify(&filesystem, pathconf_case, capabilities, valid)
}

fn classify(
    filesystem: &[u8],
    pathconf_case: libc::c_long,
    capabilities: u32,
    valid: u32,
) -> Result<NamingSemantics, NormalizationError> {
    if filesystem != b"apfs" && filesystem != b"hfs" {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }

    if valid & CASE_SENSITIVE == 0 {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }

    let metadata_is_sensitive = capabilities & CASE_SENSITIVE != 0;
    let pathconf_is_sensitive = match pathconf_case {
        0 => false,
        1 => true,
        _ => return Err(NormalizationError::UnsupportedNamingSemantics),
    };
    if metadata_is_sensitive != pathconf_is_sensitive {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }

    // Apple's volume-capability contract requires case-sensitive volumes to
    // preserve case. Treat an invalid or contradictory report as unreliable.
    if metadata_is_sensitive
        && (valid & CASE_PRESERVING == 0 || capabilities & CASE_PRESERVING == 0)
    {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }

    Ok(if metadata_is_sensitive {
        NamingSemantics::AsciiSensitive
    } else {
        NamingSemantics::AsciiInsensitive
    })
}

mod native {
    use super::*;

    #[allow(unsafe_code)]
    pub(super) fn local_filesystem(path: &CStr) -> Option<Vec<u8>> {
        let mut info = MaybeUninit::<libc::statfs>::uninit();

        // SAFETY: `path` is a live NUL-terminated string and `info` points to
        // writable storage for the complete statfs result. It is read only after
        // statfs reports success.
        if unsafe { libc::statfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
            return None;
        }
        // SAFETY: Successful statfs initializes the output structure.
        let info = unsafe { info.assume_init() };
        if !is_local_mount(info.f_flags) {
            return None;
        }

        Some(
            info.f_fstypename
                .iter()
                .copied()
                .take_while(|byte| *byte != 0)
                .map(|byte| byte as u8)
                .collect(),
        )
    }

    #[allow(unsafe_code)]
    pub(super) fn case_sensitivity(path: &CStr) -> Option<libc::c_long> {
        // SAFETY: `path` is a live NUL-terminated string. The requested name is
        // Darwin's `_PC_CASE_SENSITIVE` pathconf selector.
        let value = unsafe { libc::pathconf(path.as_ptr(), libc::_PC_CASE_SENSITIVE) };
        (value == 0 || value == 1).then_some(value)
    }

    #[allow(unsafe_code)]
    pub(super) fn volume_capabilities(path: &CStr) -> Option<(u32, u32)> {
        let mut attributes = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: 0,
            volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_CAPABILITIES,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        // The returned buffer is a u32 byte count followed by the two four-word
        // arrays in vol_capabilities_attr_t (capabilities, then valid).
        let mut buffer = [0_u32; 1 + 2 * 4];

        // SAFETY: `path` is NUL-terminated, `attributes` follows Darwin's attrlist
        // layout, and `buffer` is aligned writable storage large enough for the
        // requested fixed-size capability attribute and its leading byte count.
        let result = unsafe {
            libc::getattrlist(
                path.as_ptr(),
                (&mut attributes as *mut libc::attrlist).cast(),
                buffer.as_mut_ptr().cast(),
                size_of_val(&buffer),
                0,
            )
        };
        if result != 0 {
            return None;
        }

        let returned_size = usize::try_from(buffer[0]).ok()?;
        let required_size = size_of::<u32>() + size_of::<libc::vol_capabilities_attr_t>();
        if returned_size < required_size || returned_size > size_of_val(&buffer) {
            return None;
        }

        // Capabilities are array element zero; valid bits are element four.
        Some((buffer[1], buffer[5]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    fn capabilities(is_sensitive: bool, is_preserving: bool) -> (u32, u32) {
        let valid = CASE_SENSITIVE | CASE_PRESERVING;
        let mut flags = 0;
        if is_sensitive {
            flags |= CASE_SENSITIVE;
        }
        if is_preserving {
            flags |= CASE_PRESERVING;
        }
        (flags, valid)
    }

    #[test]
    fn rejects_mounts_without_the_local_flag() {
        assert!(is_local_mount(libc::MNT_LOCAL as u32));
        assert!(!is_local_mount(0));
    }

    #[test]
    fn recognizes_case_sensitive_apfs_using_ascii_semantics() {
        let (flags, valid) = capabilities(true, true);
        assert_eq!(
            classify(b"apfs", 1, flags, valid),
            Ok(NamingSemantics::AsciiSensitive)
        );
    }

    #[test]
    fn recognizes_case_insensitive_hfs_using_ascii_semantics() {
        let (flags, valid) = capabilities(false, true);
        assert_eq!(
            classify(b"hfs", 0, flags, valid),
            Ok(NamingSemantics::AsciiInsensitive)
        );
    }

    #[test]
    fn rejects_unknown_filesystems_even_when_case_metadata_is_present() {
        let (flags, valid) = capabilities(true, true);
        assert_eq!(
            classify(b"unknown", 1, flags, valid),
            Err(NormalizationError::UnsupportedNamingSemantics)
        );
    }

    #[test]
    fn rejects_missing_or_disagreeing_case_metadata() {
        let (flags, valid) = capabilities(true, true);
        assert_eq!(
            classify(b"apfs", 1, flags, valid & !CASE_SENSITIVE),
            Err(NormalizationError::UnsupportedNamingSemantics)
        );
        assert_eq!(
            classify(b"apfs", 0, flags, valid),
            Err(NormalizationError::UnsupportedNamingSemantics)
        );
        assert_eq!(
            classify(b"apfs", 2, flags, valid),
            Err(NormalizationError::UnsupportedNamingSemantics)
        );
    }

    #[test]
    fn rejects_case_sensitive_metadata_that_does_not_preserve_case() {
        let (flags, valid) = capabilities(true, false);
        assert_eq!(
            classify(b"hfs", 1, flags, valid),
            Err(NormalizationError::UnsupportedNamingSemantics)
        );
    }

    #[test]
    fn reads_naming_semantics_from_the_real_local_root_volume() {
        assert!(naming(Path::new("/")).is_ok());
    }

    #[test]
    fn naming_does_not_accept_nul_in_a_path() {
        let path = OsStr::from_bytes(b"/tmp\0invalid");
        assert_eq!(
            naming(Path::new(path)),
            Err(NormalizationError::UnsupportedNamingSemantics)
        );
    }
}
