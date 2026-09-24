#![doc = include_str!("../README.md")]

mod ast;
mod error;
mod parser;

pub use ast::{Effect, ParsedPolicy, PolicyDefaults, PolicyRule, ResourceSelector, SourceLocation};
pub use error::{ParseError, ParseErrorKind};
pub use parser::parse_policy;

/// Maximum UTF-8 input length in bytes.
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
/// Maximum number of nested YAML mappings and sequences.
pub const MAX_NESTING_DEPTH: usize = 64;
