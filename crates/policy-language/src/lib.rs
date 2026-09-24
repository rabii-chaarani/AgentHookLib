#![doc = include_str!("../README.md")]

mod ast;
mod error;
mod parser;
mod selectors;
mod validation;
mod validation_error;

pub use ast::{Effect, ParsedPolicy, PolicyDefaults, PolicyRule, ResourceSelector, SourceLocation};
pub use error::{ParseError, ParseErrorKind};
pub use parser::parse_policy;
pub use selectors::{
    CommandSelector, NetworkHost, NetworkSelector, PathPattern, PathSegment, PathToken,
    ValidatedResourceSelector,
};
pub use validation::{ValidatedPolicy, ValidatedRule, validate_policy};
pub use validation_error::{ValidationError, ValidationErrorKind};

/// Maximum UTF-8 input length in bytes.
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
/// Maximum number of nested YAML mappings and sequences.
pub const MAX_NESTING_DEPTH: usize = 64;
