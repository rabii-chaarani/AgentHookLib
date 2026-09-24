#![forbid(unsafe_code)]

use super::NormalizationError;
use policy_core::FileResource;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// Whether the final path component must exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRequirement {
    /// Resolve an existing target, following final symbolic links.
    Existing,
    /// Permit one missing final name beneath an existing directory.
    AllowMissingLeaf,
}

/// Verified naming behavior of an existing parent directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NamingSemantics {
    /// Native code units compare exactly, with no Unicode normalization.
    Exact,
    /// ASCII names compare exactly; non-ASCII equivalence is not established.
    AsciiSensitive,
    /// ASCII letter case is ignored; non-ASCII equivalence is not established.
    AsciiInsensitive,
}

impl NamingSemantics {
    /// Produce the same key for resource names and future selector literals.
    ///
    /// This accepts a single literal name, never a path or glob. It neither
    /// interprets wildcard syntax nor substitutes a Unicode folding algorithm.
    pub fn name_key(self, name: &OsStr) -> Result<OsString, NormalizationError> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.as_encoded_bytes().contains(&0)
            || name.as_encoded_bytes().contains(&b'/')
            || (cfg!(windows) && name.as_encoded_bytes().contains(&b'\\'))
        {
            return Err(NormalizationError::InvalidPath);
        }
        match self {
            Self::Exact => Ok(name.to_owned()),
            Self::AsciiSensitive | Self::AsciiInsensitive => {
                let text = name
                    .to_str()
                    .filter(|s| s.is_ascii())
                    .ok_or(NormalizationError::UnsupportedNamingSemantics)?;
                Ok(if self == Self::AsciiInsensitive {
                    text.to_ascii_lowercase().into()
                } else {
                    name.to_owned()
                })
            }
        }
    }
}

/// One canonical path component and the naming rules of its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingComponent {
    pub(super) name: OsString,
    pub(super) semantics: NamingSemantics,
}
impl NamingComponent {
    /// Return the native spelling (stored spelling for existing entries).
    pub fn name(&self) -> &OsStr {
        &self.name
    }
    /// Return the verified semantics for this name's parent directory.
    pub const fn semantics(&self) -> NamingSemantics {
        self.semantics
    }
}

/// A canonical path comparison key; hard-link names remain distinct.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PathIdentity {
    pub(super) root: PathBuf,
    pub(super) components: Vec<(NamingSemantics, OsString)>,
}

/// An immutable normalization result, compared by canonical path identity.
#[derive(Debug, Clone)]
pub struct NormalizedPath {
    pub(super) absolute: PathBuf,
    pub(super) relative: Option<PathBuf>,
    pub(super) exists: bool,
    pub(super) components: Vec<NamingComponent>,
    pub(super) identity: PathIdentity,
}
impl PartialEq for NormalizedPath {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}
impl Eq for NormalizedPath {}
impl NormalizedPath {
    /// Return the canonical native absolute path, not an execution capability.
    pub fn absolute_path(&self) -> &Path {
        &self.absolute
    }
    /// Return the repository-relative path, `.` for its root, or `None` outside.
    pub fn repository_relative(&self) -> Option<&Path> {
        self.relative.as_deref()
    }
    /// Whether this identity lies outside the established repository root.
    pub fn is_outside_repository(&self) -> bool {
        self.relative.is_none()
    }
    /// Whether the resolved target existed when normalization inspected it.
    pub const fn exists(&self) -> bool {
        self.exists
    }
    /// Return per-component naming semantics, from filesystem root to target.
    pub fn components(&self) -> &[NamingComponent] {
        &self.components
    }
    /// Return a comparable identity that never compares paths by display text.
    pub fn identity(&self) -> &PathIdentity {
        &self.identity
    }
}

/// A normalized source and optional rename destination; never a partial pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedFileResource {
    pub(super) source: NormalizedPath,
    pub(super) destination: Option<NormalizedPath>,
}
impl NormalizedFileResource {
    /// Return the normalized source path.
    pub fn path(&self) -> &NormalizedPath {
        &self.source
    }
    /// Return the normalized rename destination, if applicable.
    pub fn destination(&self) -> Option<&NormalizedPath> {
        self.destination.as_ref()
    }
    /// Convert to the structural core contract, explicitly discarding metadata.
    pub fn to_file_resource(&self) -> Result<FileResource, policy_core::ValidationError> {
        FileResource::new(
            self.source.absolute.clone(),
            self.destination.as_ref().map(|p| p.absolute.clone()),
        )
    }
}
