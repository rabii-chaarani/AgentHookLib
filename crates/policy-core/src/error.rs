//! Structural validation and engine-neutral authorization failures.

use std::{error::Error, fmt, path::Path};

use crate::{Action, ResourceKind};

/// A malformed contract value. Errors deliberately omit input contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// A required textual value is empty or entirely whitespace.
    Blank {
        /// The contract field that failed validation.
        field: &'static str,
    },
    /// A required path is empty.
    EmptyPath {
        /// The contract field that failed validation.
        field: &'static str,
    },
    /// A context path is not absolute on the host platform.
    PathNotAbsolute {
        /// The contract field that failed validation.
        field: &'static str,
    },
    /// A command has no executable.
    EmptyExecutable,
    /// An explicitly supplied network port is zero.
    ZeroPort,
    /// The action cannot apply to the resource family.
    IncompatibleActionResource {
        /// The requested action.
        action: Action,
        /// The supplied resource family.
        resource: ResourceKind,
    },
    /// A rename lacks its destination.
    MissingRenameDestination,
    /// A non-rename action carries a rename destination.
    UnexpectedRenameDestination,
    /// A decision has no explanation.
    MissingExplanation,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank { field } => write!(f, "{field} must not be blank"),
            Self::EmptyPath { field } => write!(f, "{field} must not be empty"),
            Self::PathNotAbsolute { field } => write!(f, "{field} must be absolute"),
            Self::EmptyExecutable => f.write_str("command executable must not be empty"),
            Self::ZeroPort => f.write_str("network port must not be zero"),
            Self::IncompatibleActionResource { action, resource } => {
                write!(
                    f,
                    "action {action:?} is incompatible with resource {resource:?}"
                )
            }
            Self::MissingRenameDestination => f.write_str("rename requires a destination"),
            Self::UnexpectedRenameDestination => f.write_str("only rename accepts a destination"),
            Self::MissingExplanation => f.write_str("a decision requires an explanation"),
        }
    }
}

impl Error for ValidationError {}

pub(crate) fn nonblank(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::Blank { field })
    } else {
        Ok(())
    }
}

pub(crate) fn nonempty_path(value: &Path, field: &'static str) -> Result<(), ValidationError> {
    if value.as_os_str().is_empty() {
        Err(ValidationError::EmptyPath { field })
    } else {
        Ok(())
    }
}

pub(crate) fn absolute_path(value: &Path, field: &'static str) -> Result<(), ValidationError> {
    nonempty_path(value, field)?;
    if value.is_absolute() {
        Ok(())
    } else {
        Err(ValidationError::PathNotAbsolute { field })
    }
}

/// Engine-neutral categories of authorization failure, distinct from decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationErrorKind {
    /// The request cannot be interpreted by the evaluator.
    InvalidRequest,
    /// Required policy artifacts could not be loaded or prepared.
    PolicyUnavailable,
    /// Policy evaluation failed.
    EvaluationFailed,
    /// The requested operation is unsupported.
    UnsupportedOperation,
}

/// An authorization failure; callers must not treat an error as permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationError {
    kind: AuthorizationErrorKind,
    diagnostic: String,
}

impl AuthorizationError {
    /// Construct a failure with a nonblank, caller-sanitized diagnostic.
    pub fn new(
        kind: AuthorizationErrorKind,
        diagnostic: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let diagnostic = diagnostic.into();
        nonblank(&diagnostic, "diagnostic")?;
        Ok(Self { kind, diagnostic })
    }

    /// Return the engine-neutral failure category.
    pub fn kind(&self) -> AuthorizationErrorKind {
        self.kind
    }

    /// Return the diagnostic exactly as supplied.
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.diagnostic)
    }
}

impl Error for AuthorizationError {}
