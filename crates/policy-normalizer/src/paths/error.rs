#![forbid(unsafe_code)]

use std::{error::Error, fmt, io};

/// A normalization failure, deliberately containing no input path or OS text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NormalizationError {
    /// Repository root or operation cwd is not a resolvable directory.
    InvalidContext,
    /// The supplied path or resource shape is invalid.
    InvalidPath,
    /// A target required to exist does not exist.
    MissingTarget,
    /// An intermediate path component does not exist.
    MissingParent,
    /// An intermediate path component is not a directory.
    NotDirectory,
    /// Filesystem access failed; no raw operating-system diagnostic is retained.
    Inaccessible,
    /// A symbolic link or reparse point cannot be resolved to an existing target.
    UnresolvedLink,
    /// Directory naming rules or a name's equivalence cannot be established.
    UnsupportedNamingSemantics,
    /// A Windows path namespace or legacy spelling is not supported.
    UnsupportedWindowsNamespace,
    /// The host platform has no normalization backend.
    UnsupportedPlatform,
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidContext => "normalization context is not a resolvable directory",
            Self::InvalidPath => "resource path or operation shape is invalid",
            Self::MissingTarget => "required target does not exist",
            Self::MissingParent => "target parent does not exist",
            Self::NotDirectory => "path ancestor is not a directory",
            Self::Inaccessible => "filesystem metadata is inaccessible",
            Self::UnresolvedLink => "link identity cannot be resolved",
            Self::UnsupportedNamingSemantics => "filesystem naming semantics are unsupported",
            Self::UnsupportedWindowsNamespace => {
                "Windows path namespace or spelling is unsupported"
            }
            Self::UnsupportedPlatform => "host platform is unsupported",
        })
    }
}

impl Error for NormalizationError {}

pub(super) fn io_error(error: io::Error) -> NormalizationError {
    match error.kind() {
        io::ErrorKind::NotFound => NormalizationError::MissingTarget,
        io::ErrorKind::NotADirectory => NormalizationError::NotDirectory,
        _ => NormalizationError::Inaccessible,
    }
}
