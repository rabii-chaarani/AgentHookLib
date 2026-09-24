use crate::SourceLocation;
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// Stable categories for structural parsing failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// The document exceeds the input byte limit.
    InputTooLarge,
    /// YAML collection nesting exceeds the depth limit.
    NestingTooDeep,
    /// The YAML scanner or parser rejected the input.
    InvalidYaml,
    /// No non-null document was supplied.
    EmptyDocument,
    /// More than one YAML document was supplied.
    MultipleDocuments,
    /// An explicit YAML tag was supplied.
    ExplicitTag,
    /// An anchor or alias was supplied.
    AnchorOrAlias,
    /// A YAML merge key was supplied.
    MergeKey,
    /// A mapping key is not a textual scalar.
    InvalidMappingKey,
    /// Two decoded mapping keys are equal.
    DuplicateKey,
    /// A field is not supported by its enclosing type.
    UnknownField,
    /// A required field is absent.
    MissingField,
    /// A value has the wrong shape, is null, or is an invalid integer.
    InvalidType,
    /// An effect name is unknown.
    UnknownEffect,
    /// An action name is unknown.
    UnknownAction,
    /// A resource kind is unknown.
    UnknownResourceKind,
}

/// A source-located error; no input values or YAML snippets are displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub(crate) kind: ParseErrorKind,
    pub(crate) source: PathBuf,
    pub(crate) location: SourceLocation,
    pub(crate) field_path: Option<String>,
    pub(crate) original_location: Option<SourceLocation>,
}

impl ParseError {
    /// Return the structural error category.
    pub const fn kind(&self) -> ParseErrorKind {
        self.kind
    }
    /// Return the caller-supplied policy source label.
    pub fn source(&self) -> &Path {
        &self.source
    }
    /// Return the offending location, or enclosing mapping for missing fields.
    pub const fn location(&self) -> SourceLocation {
        self.location
    }
    /// Return the field path, when it could be determined.
    pub fn field_path(&self) -> Option<&str> {
        self.field_path.as_deref()
    }
    /// Return the first occurrence of a duplicate key.
    pub const fn original_location(&self) -> Option<SourceLocation> {
        self.original_location
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {:?}",
            self.source.display(),
            self.location.line(),
            self.location.column(),
            self.kind
        )?;
        if let Some(path) = &self.field_path {
            write!(f, " at {path}")?;
        }
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

impl std::error::Error for ParseError {}
