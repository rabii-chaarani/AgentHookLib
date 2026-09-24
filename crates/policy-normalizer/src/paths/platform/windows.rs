//! Read-only Windows path parsing and filesystem metadata access.
//!
//! The only unsafe operations in this module are small calls to the Windows
//! API. Their pointers refer to live, correctly sized buffers for the duration
//! of each call, and returned handles are closed by `Handle`.

use std::{
    ffi::{OsStr, OsString},
    fs::{self, Metadata},
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
        FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FileAttributeTagInfo, FileCaseSensitiveInfo, GetDriveTypeW, GetFileInformationByHandleEx,
        GetLongPathNameW, GetVolumeInformationW, GetVolumePathNameW, OPEN_EXISTING,
    },
    System::{
        SystemServices::{
            FILE_CS_FLAG_CASE_SENSITIVE_DIR, IO_REPARSE_TAG_MOUNT_POINT, IO_REPARSE_TAG_SYMLINK,
        },
        WindowsProgramming::{
            DRIVE_FIXED, DRIVE_NO_ROOT_DIR, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
            DRIVE_UNKNOWN,
        },
    },
};

use super::super::{NamingSemantics, NormalizationError, error::io_error};

const MAX_WIDE_PATH: usize = 32_767;

/// Inspect the existing directory's local NTFS case-sensitivity setting.
pub(in crate::paths) fn naming(dir: &Path) -> Result<NamingSemantics, NormalizationError> {
    let canonical = fs::canonicalize(dir).map_err(io_error)?;
    if !fs::metadata(&canonical).map_err(io_error)?.is_dir() {
        return Err(NormalizationError::NotDirectory);
    }

    let path = nul_terminated(canonical.as_os_str())?;
    let volume_root =
        volume_path(&path).map_err(|_| NormalizationError::UnsupportedNamingSemantics)?;

    // UNC roots and mapped network shares are deliberately outside the
    // supported local-filesystem contract.
    let drive_type = get_drive_type(&volume_root);
    if matches!(drive_type, DRIVE_REMOTE | DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR) {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }
    if !matches!(drive_type, DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK) {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }

    if !is_ntfs(&volume_root) {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }

    let handle = open_handle(&canonical, FILE_FLAG_BACKUP_SEMANTICS)?;
    let info =
        query_case_info(handle.0).map_err(|_| NormalizationError::UnsupportedNamingSemantics)?;
    if info.Flags & !FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0 {
        return Err(NormalizationError::UnsupportedNamingSemantics);
    }
    if info.Flags & FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0 {
        Ok(NamingSemantics::Exact)
    } else {
        Ok(NamingSemantics::AsciiInsensitive)
    }
}

/// Split a Windows path into a canonical root and lossless raw components.
pub(in crate::paths) fn parse(
    path: &Path,
    cwd: &Path,
) -> Result<(PathBuf, Vec<OsString>), NormalizationError> {
    let input = wide(path.as_os_str())?;
    if input.is_empty() {
        return Err(NormalizationError::InvalidPath);
    }

    let parsed = match classify(&input)? {
        InputKind::Absolute(parsed) => parsed,
        InputKind::RootRelative(segments, trailing) => {
            let base = parse_absolute(&wide(cwd.as_os_str())?)
                .map_err(|_| NormalizationError::InvalidContext)?;
            if !is_drive_root(&base.root) {
                return Err(NormalizationError::InvalidContext);
            }
            ParsedPath {
                root: base.root,
                segments: append_trailing_marker(segments, trailing),
            }
        }
        InputKind::Relative(segments, trailing) => {
            let mut base = parse_absolute(&wide(cwd.as_os_str())?)
                .map_err(|_| NormalizationError::InvalidContext)?;
            base.segments
                .extend(append_trailing_marker(segments, trailing));
            base
        }
    };

    Ok((
        PathBuf::from(OsString::from_wide(&parsed.root)),
        parsed
            .segments
            .into_iter()
            .map(|part| OsString::from_wide(&part))
            .collect(),
    ))
}

/// Return the immediate target of a supported symlink or junction.
///
/// The shared resolver follows this target recursively and requires it to exist.
pub(in crate::paths) fn redirect(
    path: &Path,
    metadata: &Metadata,
) -> Result<Option<PathBuf>, NormalizationError> {
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return Ok(None);
    }

    let handle = open_handle(
        path,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
    )
    .map_err(|_| NormalizationError::UnresolvedLink)?;
    let info = query_reparse_info(handle.0).map_err(|_| NormalizationError::UnresolvedLink)?;
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return Err(NormalizationError::UnresolvedLink);
    }
    if !matches!(
        info.ReparseTag,
        IO_REPARSE_TAG_SYMLINK | IO_REPARSE_TAG_MOUNT_POINT
    ) {
        return Err(NormalizationError::UnresolvedLink);
    }

    fs::read_link(path)
        .map(Some)
        .map_err(|_| NormalizationError::UnresolvedLink)
}

/// Return an existing entry's long, stored spelling beneath its unchanged parent.
pub(in crate::paths) fn stored_name(
    parent: &Path,
    name: &OsStr,
    semantics: NamingSemantics,
) -> Result<OsString, NormalizationError> {
    let key = semantics.name_key(name)?;
    if let Some(stored) = find_stored_child(parent, semantics, &key)? {
        return Ok(stored);
    }

    // The directory scan above verifies stored case and long names. Win32 is
    // consulted only for aliases such as an 8.3 short name that enumeration
    // does not expose as a separate entry.
    let mut candidate = parent.join(name);
    let candidate_wide = nul_terminated(candidate.as_os_str())?;
    let long_path = get_long_path(&candidate_wide)?;
    candidate = PathBuf::from(OsString::from_wide(&long_path));

    // GetLongPathNameW can also rewrite parent components. The normalizer has
    // already canonicalized them, so accept only a result with the same exact
    // parent spelling; otherwise the final-name lookup crossed its boundary.
    if candidate.parent() != Some(parent) {
        return Err(NormalizationError::Inaccessible);
    }
    let alias_target = candidate
        .file_name()
        .map(OsStr::to_owned)
        .ok_or(NormalizationError::InvalidPath)?;
    let alias_key = semantics.name_key(&alias_target)?;
    find_stored_child(parent, semantics, &alias_key)?.ok_or(NormalizationError::Inaccessible)
}

fn find_stored_child(
    parent: &Path,
    semantics: NamingSemantics,
    key: &OsStr,
) -> Result<Option<OsString>, NormalizationError> {
    for entry in fs::read_dir(parent).map_err(io_error)? {
        let stored = entry.map_err(io_error)?.file_name();
        if semantics
            .name_key(&stored)
            .is_ok_and(|candidate| candidate == key)
        {
            return Ok(Some(stored));
        }
    }
    Ok(None)
}

#[derive(Debug)]
struct ParsedPath {
    root: Vec<u16>,
    segments: Vec<Vec<u16>>,
}

enum InputKind {
    Absolute(ParsedPath),
    RootRelative(Vec<Vec<u16>>, bool),
    Relative(Vec<Vec<u16>>, bool),
}

fn classify(input: &[u16]) -> Result<InputKind, NormalizationError> {
    if input.is_empty() || input.contains(&0) {
        return Err(NormalizationError::InvalidPath);
    }

    if starts_with(
        input,
        &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16],
    ) {
        let body = &input[4..];
        if starts_ascii_case_insensitive(
            body,
            &[b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16],
        ) {
            validate_extended_unc(&body[4..])?;
            return parse_unc_tail(&body[4..]).map(InputKind::Absolute);
        }
        if body.len() >= 3 && is_ascii_alpha(body[0]) && body[1] == b':' as u16 && is_sep(body[2]) {
            validate_extended_disk(&body[3..])?;
            let mut disk = body.to_vec();
            canonicalize_drive_letter(&mut disk);
            return parse_drive(&disk).map(InputKind::Absolute);
        }
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }

    if starts_with(
        input,
        &[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16],
    ) || starts_ascii_case_insensitive(
        input,
        &[b'\\' as u16, b'?' as u16, b'?' as u16, b'\\' as u16],
    ) {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    if input.len() >= 2 && is_sep(input[0]) && is_sep(input[1]) {
        return parse_unc_tail(&input[2..]).map(InputKind::Absolute);
    }
    if input.len() >= 2 && is_ascii_alpha(input[0]) && input[1] == b':' as u16 {
        if input.len() < 3 || !is_sep(input[2]) {
            return Err(NormalizationError::UnsupportedWindowsNamespace);
        }
        let mut disk = input.to_vec();
        canonicalize_drive_letter(&mut disk);
        return parse_drive(&disk).map(InputKind::Absolute);
    }
    if is_sep(input[0]) {
        let (segments, trailing) = split_segments(&input[1..])?;
        return Ok(InputKind::RootRelative(segments, trailing));
    }
    let (segments, trailing) = split_segments(input)?;
    Ok(InputKind::Relative(segments, trailing))
}

fn parse_absolute(input: &[u16]) -> Result<ParsedPath, NormalizationError> {
    match classify(input)? {
        InputKind::Absolute(path) => Ok(path),
        _ => Err(NormalizationError::InvalidContext),
    }
}

fn parse_drive(input: &[u16]) -> Result<ParsedPath, NormalizationError> {
    if input.len() < 3 || !is_ascii_alpha(input[0]) || input[1] != b':' as u16 || !is_sep(input[2])
    {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    let mut root = input[..3].to_vec();
    root[0] = ascii_upper_unit(input[0]);
    root[2] = b'\\' as u16;
    let (segments, trailing) = split_segments(&input[3..])?;
    let trailing =
        trailing || (segments.is_empty() && input.last().is_some_and(|unit| is_sep(*unit)));
    Ok(ParsedPath {
        root,
        segments: append_trailing_marker(segments, trailing),
    })
}

fn parse_unc_tail(tail: &[u16]) -> Result<ParsedPath, NormalizationError> {
    if tail.is_empty() || is_sep(tail[0]) {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    let server_end = tail
        .iter()
        .position(|unit| is_sep(*unit))
        .ok_or(NormalizationError::UnsupportedWindowsNamespace)?;
    let server = &tail[..server_end];
    let after_server = &tail[server_end + 1..];
    if server.is_empty() || after_server.is_empty() || is_sep(after_server[0]) {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    let share_end = after_server
        .iter()
        .position(|unit| is_sep(*unit))
        .unwrap_or(after_server.len());
    let share = &after_server[..share_end];
    if share.is_empty() {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    validate_component(server, false)?;
    validate_component(share, false)?;

    let mut root = vec![b'\\' as u16, b'\\' as u16];
    root.extend_from_slice(server);
    root.push(b'\\' as u16);
    root.extend_from_slice(share);
    root.push(b'\\' as u16);

    let remainder = if share_end == after_server.len() {
        &[][..]
    } else {
        &after_server[share_end + 1..]
    };
    let trailing = tail.last().is_some_and(|unit| is_sep(*unit));
    let (segments, _) = split_segments(remainder)?;
    Ok(ParsedPath {
        root,
        segments: append_trailing_marker(segments, trailing),
    })
}

fn validate_extended_disk(suffix: &[u16]) -> Result<(), NormalizationError> {
    if suffix.first() == Some(&(b'\\' as u16))
        || suffix.contains(&(b'/' as u16))
        || suffix
            .windows(2)
            .any(|pair| pair == [b'\\' as u16, b'\\' as u16])
        || suffix
            .split(|unit| *unit == b'\\' as u16)
            .any(|part| part == [b'.' as u16] || part == [b'.' as u16, b'.' as u16])
    {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    Ok(())
}

fn validate_extended_unc(tail: &[u16]) -> Result<(), NormalizationError> {
    if tail.contains(&(b'/' as u16))
        || tail
            .windows(2)
            .any(|pair| pair == [b'\\' as u16, b'\\' as u16])
    {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    let mut separators = tail
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| (*unit == b'\\' as u16).then_some(index));
    separators
        .next()
        .ok_or(NormalizationError::UnsupportedWindowsNamespace)?;
    let share_separator = separators.next();
    let suffix_start = share_separator.map_or(tail.len(), |index| index + 1);
    if tail[suffix_start..]
        .split(|unit| *unit == b'\\' as u16)
        .any(|part| part == [b'.' as u16] || part == [b'.' as u16, b'.' as u16])
    {
        return Err(NormalizationError::UnsupportedWindowsNamespace);
    }
    Ok(())
}

fn split_segments(input: &[u16]) -> Result<(Vec<Vec<u16>>, bool), NormalizationError> {
    let trailing = input.last().is_some_and(|unit| is_sep(*unit));
    let mut segments = Vec::new();
    let mut start = 0;
    for (index, unit) in input.iter().enumerate() {
        if is_sep(*unit) {
            if start < index {
                let segment = input[start..index].to_vec();
                validate_component(&segment, true)?;
                segments.push(segment);
            }
            start = index + 1;
        }
    }
    if start < input.len() {
        let segment = input[start..].to_vec();
        validate_component(&segment, true)?;
        segments.push(segment);
    }
    Ok((segments, trailing))
}

fn validate_component(
    component: &[u16],
    allow_dot_segments: bool,
) -> Result<(), NormalizationError> {
    if component.is_empty() {
        return Err(NormalizationError::InvalidPath);
    }
    if allow_dot_segments && (component == [b'.' as u16] || component == [b'.' as u16, b'.' as u16])
    {
        return Ok(());
    }
    if component.iter().any(|unit| {
        *unit == b':' as u16
            || *unit == b'/' as u16
            || *unit == b'\\' as u16
            || *unit == b'<' as u16
            || *unit == b'>' as u16
            || *unit == b'"' as u16
            || *unit == b'|' as u16
            || *unit == b'?' as u16
            || *unit == b'*' as u16
            || (*unit > 0 && *unit < 32)
    }) || component
        .last()
        .is_some_and(|unit| matches!(*unit, 0x20 | 0x2e))
    {
        return Err(NormalizationError::InvalidPath);
    }
    if is_legacy_device_name(component) {
        return Err(NormalizationError::InvalidPath);
    }
    Ok(())
}

fn is_legacy_device_name(component: &[u16]) -> bool {
    let stem_end = component
        .iter()
        .position(|unit| *unit == b'.' as u16)
        .unwrap_or(component.len());
    let mut stem = component[..stem_end].to_vec();
    while stem.last().is_some_and(|unit| matches!(*unit, 0x20 | 0x2e)) {
        stem.pop();
    }
    let ascii = stem
        .iter()
        .map(|unit| ascii_upper_unit(*unit))
        .collect::<Vec<_>>();
    let fixed = [
        b"CON".as_slice(),
        b"PRN",
        b"AUX",
        b"NUL",
        b"CONIN$",
        b"CONOUT$",
        b"CLOCK$",
    ];
    if fixed
        .iter()
        .any(|name| ascii == name.iter().map(|byte| u16::from(*byte)).collect::<Vec<_>>())
    {
        return true;
    }
    if ascii.len() == 4
        && (ascii[..3] == [b'C' as u16, b'O' as u16, b'M' as u16]
            || ascii[..3] == [b'L' as u16, b'P' as u16, b'T' as u16])
    {
        return matches!(ascii[3], 0x31..=0x39 | 0x00b9 | 0x00b2 | 0x00b3);
    }
    false
}

fn append_trailing_marker(mut segments: Vec<Vec<u16>>, trailing: bool) -> Vec<Vec<u16>> {
    if trailing {
        segments.push(vec![b'.' as u16]);
    }
    segments
}

fn canonicalize_drive_letter(path: &mut [u16]) {
    if !path.is_empty() {
        path[0] = ascii_upper_unit(path[0]);
    }
}

fn is_drive_root(root: &[u16]) -> bool {
    root.len() == 3 && is_ascii_alpha(root[0]) && root[1] == b':' as u16 && is_sep(root[2])
}

fn is_sep(unit: u16) -> bool {
    unit == b'\\' as u16 || unit == b'/' as u16
}

fn is_ascii_alpha(unit: u16) -> bool {
    matches!(unit, 0x41..=0x5a | 0x61..=0x7a)
}

fn starts_with(input: &[u16], prefix: &[u16]) -> bool {
    input.starts_with(prefix)
}

fn starts_ascii_case_insensitive(input: &[u16], prefix: &[u16]) -> bool {
    input.len() >= prefix.len()
        && input
            .iter()
            .zip(prefix)
            .take(prefix.len())
            .all(|(left, right)| ascii_upper_unit(*left) == ascii_upper_unit(*right))
}

fn ascii_upper_unit(unit: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&unit) {
        unit - (b'a' as u16 - b'A' as u16)
    } else {
        unit
    }
}

fn wide_ascii_eq(input: &[u16], expected: &[u8]) -> bool {
    let Some(end) = nul_len(input) else {
        return false;
    };
    end == expected.len()
        && input[..end]
            .iter()
            .zip(expected)
            .all(|(left, right)| ascii_upper_unit(*left) == u16::from(right.to_ascii_uppercase()))
}

fn nul_len(input: &[u16]) -> Option<usize> {
    input.iter().position(|unit| *unit == 0)
}

fn wide(path: &OsStr) -> Result<Vec<u16>, NormalizationError> {
    let output = path.encode_wide().collect::<Vec<_>>();
    if output.contains(&0) {
        return Err(NormalizationError::InvalidPath);
    }
    Ok(output)
}

fn nul_terminated(path: &OsStr) -> Result<Vec<u16>, NormalizationError> {
    let mut result = wide(path)?;
    result.push(0);
    Ok(result)
}

#[allow(unsafe_code)]
fn volume_path(path: &[u16]) -> Result<Vec<u16>, NormalizationError> {
    let mut output = vec![0u16; MAX_WIDE_PATH];
    // SAFETY: `path` is NUL-terminated and `output` is writable for its
    // advertised capacity; both buffers live for the synchronous call.
    let succeeded =
        unsafe { GetVolumePathNameW(path.as_ptr(), output.as_mut_ptr(), output.len() as u32) };
    if succeeded == 0 {
        return Err(NormalizationError::Inaccessible);
    }
    let end = nul_len(&output).ok_or(NormalizationError::Inaccessible)?;
    output.truncate(end + 1);
    Ok(output)
}

#[allow(unsafe_code)]
fn get_drive_type(path: &[u16]) -> u32 {
    // SAFETY: `path` is a live NUL-terminated volume path.
    unsafe { GetDriveTypeW(path.as_ptr()) }
}

#[allow(unsafe_code)]
fn is_ntfs(root: &[u16]) -> bool {
    let mut fs_name = [0u16; 64];
    // SAFETY: `root` is NUL-terminated; all optional outputs are null, and
    // `fs_name` is writable for the supplied count.
    let succeeded = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    };
    succeeded != 0 && wide_ascii_eq(&fs_name, b"NTFS")
}

#[allow(unsafe_code)]
fn query_case_info(handle: HANDLE) -> Result<FILE_CASE_SENSITIVE_INFO, NormalizationError> {
    let mut info = FILE_CASE_SENSITIVE_INFO::default();
    // SAFETY: `info` is correctly sized and writable, and the owned handle
    // remains valid until this synchronous query returns.
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileCaseSensitiveInfo,
            (&mut info as *mut FILE_CASE_SENSITIVE_INFO).cast(),
            size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
        )
    };
    if succeeded == 0 {
        Err(NormalizationError::UnsupportedNamingSemantics)
    } else {
        Ok(info)
    }
}

#[allow(unsafe_code)]
fn query_reparse_info(handle: HANDLE) -> Result<FILE_ATTRIBUTE_TAG_INFO, NormalizationError> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: `info` is correctly sized and writable, and the owned handle
    // remains valid until this synchronous query returns.
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if succeeded == 0 {
        Err(NormalizationError::UnresolvedLink)
    } else {
        Ok(info)
    }
}

#[allow(unsafe_code)]
fn open_handle(path: &Path, flags: u32) -> Result<Handle, NormalizationError> {
    let path = nul_terminated(path.as_os_str())?;
    // SAFETY: `path` is a NUL-terminated UTF-16 path and the API copies it
    // synchronously. Null security/template pointers are documented defaults.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            flags,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        Err(NormalizationError::Inaccessible)
    } else {
        Ok(Handle(handle))
    }
}

#[allow(unsafe_code)]
fn get_long_path(path: &[u16]) -> Result<Vec<u16>, NormalizationError> {
    let mut capacity = 512usize;
    loop {
        if capacity > MAX_WIDE_PATH {
            return Err(NormalizationError::Inaccessible);
        }
        let mut output = vec![0u16; capacity];
        // SAFETY: `path` is NUL-terminated; `output` has the advertised writable
        // capacity. Both buffers remain valid through the synchronous call.
        let written =
            unsafe { GetLongPathNameW(path.as_ptr(), output.as_mut_ptr(), capacity as u32) }
                as usize;
        if written == 0 {
            return Err(NormalizationError::Inaccessible);
        }
        if written < capacity {
            output.truncate(written);
            return Ok(output);
        }
        capacity = written.saturating_add(1);
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: `Handle` is constructed only from a non-null, non-invalid
        // CreateFileW result and uniquely owns that handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    fn parts(result: Vec<OsString>) -> Vec<Vec<u16>> {
        result
            .into_iter()
            .map(|part| part.encode_wide().collect())
            .collect()
    }

    #[test]
    fn drive_paths_keep_dot_segments_and_canonicalize_drive_letter() {
        let (root, components) = parse(&path(r"c:\Repo\.\src\..\file.rs"), &path(r"D:\cwd"))
            .expect("valid absolute path");
        assert_eq!(root, path(r"C:\"));
        assert_eq!(
            parts(components),
            ["Repo", ".", "src", "..", "file.rs"]
                .map(|part| part.encode_utf16().collect::<Vec<_>>())
                .to_vec()
        );
    }

    #[test]
    fn root_relative_uses_only_the_explicit_cwd_drive() {
        let (root, components) = parse(&path(r"\outside\leaf"), &path(r"d:\repo\child"))
            .expect("valid root-relative path");
        assert_eq!(root, path(r"D:\"));
        assert_eq!(
            parts(components),
            ["outside", "leaf"]
                .map(|part| part.encode_utf16().collect::<Vec<_>>())
                .to_vec()
        );
    }

    #[test]
    fn extended_and_ordinary_unc_paths_share_the_canonical_root() {
        let ordinary =
            parse(&path(r"\\server\share\dir\leaf"), &path(r"C:\cwd")).expect("valid UNC path");
        let extended = parse(&path(r"\\?\UNC\server\share\dir\leaf"), &path(r"C:\cwd"))
            .expect("valid extended UNC path");
        assert_eq!(ordinary, extended);
        assert_eq!(ordinary.0, path(r"\\server\share\"));
    }

    #[test]
    fn trailing_separator_adds_a_directory_marker() {
        let (_, components) =
            parse(&path(r"C:\repo\folder\"), &path(r"C:\cwd")).expect("valid trailing separator");
        assert_eq!(
            parts(components),
            ["repo", "folder", "."]
                .map(|part| part.encode_utf16().collect::<Vec<_>>())
                .to_vec()
        );
    }

    #[test]
    fn drive_root_trailing_separator_has_directory_marker() {
        let (root, components) = parse(&path(r"C:\"), &path(r"C:\cwd")).expect("valid drive root");
        assert_eq!(root, path(r"C:\"));
        assert_eq!(
            parts(components),
            ["."]
                .map(|part| part.encode_utf16().collect::<Vec<_>>())
                .to_vec()
        );
    }

    #[test]
    fn extended_paths_reject_dot_segments_and_repeated_separators() {
        for input in [
            r"\\?\C:\repo\.\leaf",
            r"\\?\C:\repo\..\leaf",
            r"\\?\C:\\leaf",
            r"\\?\C:\repo\\leaf",
            r"\\?\UNC\server\share\.\leaf",
            r"\\?\UNC\server\share\\leaf",
        ] {
            assert!(
                parse(&path(input), &path(r"C:\cwd")).is_err(),
                "accepted {input}"
            );
        }
    }

    #[test]
    fn root_relative_dot_directory_is_not_mistaken_for_a_device_namespace() {
        let (root, components) = parse(&path(r"\.\leaf"), &path(r"C:\cwd"))
            .expect("valid root-relative path with a dot segment");
        assert_eq!(root, path(r"C:\"));
        assert_eq!(
            parts(components),
            [".", "leaf"]
                .map(|part| part.encode_utf16().collect::<Vec<_>>())
                .to_vec()
        );
    }

    #[test]
    fn rejects_drive_relative_ads_reserved_and_legacy_trailing_names() {
        for input in [
            r"C:relative",
            r"C:\file:stream",
            r"C:\NUL.txt",
            r"C:\folder.",
            r"C:\folder ",
        ] {
            assert!(
                parse(&path(input), &path(r"C:\cwd")).is_err(),
                "accepted {input}"
            );
        }
    }

    #[test]
    fn invalid_utf16_name_units_are_preserved() {
        let isolated_surrogate = OsString::from_wide(&[0xd800]);
        let input = PathBuf::from(r"C:\repo").join(isolated_surrogate.clone());
        let (_, components) = parse(&input, &path(r"C:\cwd")).expect("UTF-16 must remain lossless");
        assert_eq!(components.last(), Some(&isolated_surrogate));
    }
}
