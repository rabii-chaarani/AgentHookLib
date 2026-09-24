#![forbid(unsafe_code)]
use super::super::{NamingSemantics, NormalizationError, error::io_error};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, Metadata},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

pub(in crate::paths) fn parse(
    path: &Path,
    cwd: &Path,
) -> Result<(PathBuf, Vec<OsString>), NormalizationError> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(NormalizationError::InvalidPath);
    }
    let mut absolute = Vec::new();
    if !path.is_absolute() {
        if !cwd.is_absolute() {
            return Err(NormalizationError::InvalidContext);
        }
        absolute.extend_from_slice(cwd.as_os_str().as_bytes());
        absolute.push(b'/');
    }
    absolute.extend_from_slice(bytes);
    let mut parts: Vec<_> = absolute
        .split(|b| *b == b'/')
        .filter(|part| !part.is_empty())
        .map(|part| OsString::from_vec(part.to_vec()))
        .collect();
    // A trailing separator requires a directory even when the leaf is absent.
    if absolute.last() == Some(&b'/') {
        parts.push(OsString::from("."));
    }
    Ok((PathBuf::from("/"), parts))
}

pub(in crate::paths) fn redirect(
    path: &Path,
    metadata: &Metadata,
) -> Result<Option<PathBuf>, NormalizationError> {
    if metadata.is_symlink() {
        fs::read_link(path)
            .map(Some)
            .map_err(|_| NormalizationError::UnresolvedLink)
    } else {
        Ok(None)
    }
}

pub(in crate::paths) fn stored_name(
    parent: &Path,
    name: &OsStr,
    semantics: NamingSemantics,
) -> Result<OsString, NormalizationError> {
    let key = semantics.name_key(name)?;
    if semantics != NamingSemantics::AsciiInsensitive {
        return Ok(name.to_owned());
    }
    for entry in fs::read_dir(parent).map_err(io_error)? {
        let name = entry.map_err(io_error)?.file_name();
        if semantics
            .name_key(&name)
            .is_ok_and(|candidate| candidate == key)
        {
            return Ok(name);
        }
    }
    Err(NormalizationError::Inaccessible)
}
