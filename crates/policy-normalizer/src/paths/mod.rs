//! Canonical path identities from explicit operation context.
//!
//! Normalization follows final links and never creates files. Missing leaves are
//! supported only beneath existing directories. Unknown naming behavior fails
//! closed. Results are snapshots: they do not authorize an operation, model the
//! entry semantics of unlink/rename, or prevent execution-time filesystem races.

mod error;
mod platform;
mod resolver;
mod types;

pub use error::NormalizationError;
pub use resolver::{normalize_file_resource, normalize_path};
pub use types::{
    NamingComponent, NamingSemantics, NormalizedFileResource, NormalizedPath, PathIdentity,
    PathRequirement,
};
