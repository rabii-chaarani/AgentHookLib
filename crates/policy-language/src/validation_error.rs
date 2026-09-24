use crate::{ParsedPolicy, SourceLocation};
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// Categories of semantic failure, distinct from structural parsing errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationErrorKind {
    /// Only version 1 is supported.
    UnsupportedVersion,
    /// An identifier is blank or contains a control character.
    InvalidRuleId,
    /// A rule ID already appeared in this document.
    DuplicateRuleId,
    /// A rule must name at least one action.
    EmptyActions,
    /// An action and selector belong to different resource families.
    IncompatibleAction,
    /// A file or Git selector violates the shared path-pattern grammar.
    InvalidPathPattern,
    /// An executable is blank or contains NUL.
    InvalidExecutable,
    /// A literal argument contains NUL.
    InvalidArgument,
    /// A host is not a supported exact DNS name or IP literal.
    InvalidHost,
    /// Port zero is not a supported destination.
    InvalidPort,
}

/// A deterministic, source-located semantic error; display omits scalar values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    kind: ValidationErrorKind,
    source: PathBuf,
    field_path: String,
    location: SourceLocation,
    original_location: Option<SourceLocation>,
}

impl ValidationError {
    /// Return the semantic failure category.
    pub const fn kind(&self) -> ValidationErrorKind {
        self.kind
    }
    /// Return the diagnostic source label, without resolving it.
    pub fn source(&self) -> &Path {
        &self.source
    }
    /// Return the offending field's path.
    pub fn field_path(&self) -> &str {
        &self.field_path
    }
    /// Return the offending value's one-based source location.
    pub const fn location(&self) -> SourceLocation {
        self.location
    }
    /// Return the first rule ID's location for duplicate identifiers.
    pub const fn original_location(&self) -> Option<SourceLocation> {
        self.original_location
    }

    pub(super) fn new(
        policy: &ParsedPolicy,
        kind: ValidationErrorKind,
        field_path: String,
    ) -> Self {
        Self {
            kind,
            source: policy.source().into(),
            // ParsedPolicy is constructed only by the parser, which locates all
            // fields validated here. Keep a document fallback for future fields.
            location: policy
                .location(&field_path)
                .unwrap_or(SourceLocation { line: 1, column: 1 }),
            field_path,
            original_location: None,
        }
    }

    pub(super) fn with_original(mut self, location: Option<SourceLocation>) -> Self {
        self.original_location = location;
        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {:?} at {}",
            self.source.display(),
            self.location.line(),
            self.location.column(),
            self.kind,
            self.field_path
        )?;
        if let Some(first) = self.original_location {
            write!(
                f,
                " (first occurrence at {}:{})",
                first.line(),
                first.column()
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}
