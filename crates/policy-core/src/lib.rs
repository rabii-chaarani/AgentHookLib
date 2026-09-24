//! Dependency-free, agent-neutral authorization contracts.
//!
//! Construction validates structure only: a valid request is neither permission
//! to proceed nor proof of canonical filesystem identity. Constructors preserve
//! supplied values and perform no I/O, path normalization, command parsing, or
//! host resolution. Callers must normalize resources before policy evaluation.
//! Execution and sandboxing belong to the coding-agent runtime.
//!
//! ```
//! use policy_core::{AgentKind, Principal};
//!
//! let principal = Principal::new(AgentKind::Codex, "session-42")?;
//! assert_eq!(principal.session_id(), "session-42");
//! # Ok::<(), policy_core::ValidationError>(())
//! ```
//!
//! Validated aggregates cannot be mutated through public fields:
//!
//! ```compile_fail
//! use policy_core::{AgentKind, Principal};
//! let mut principal = Principal::new(AgentKind::Claude, "session-42").unwrap();
//! principal.session_id = String::new();
//! ```

mod action;
mod context;
mod decision;
mod error;
mod principal;
mod request;
mod resource;
mod service;

pub use action::{Action, CommandAction, FileAction, GitAction, NetworkAction, ResourceKind};
pub use context::Context;
pub use decision::{
    DecisionKind, DecisionReason, PolicyDecision, PolicyLayer, PolicySource, RuleReference,
};
pub use error::{AuthorizationError, AuthorizationErrorKind, ValidationError};
pub use principal::{AgentKind, Principal};
pub use request::AuthorizationRequest;
pub use resource::{CommandResource, FileResource, GitResource, NetworkResource, Resource};
pub use service::PolicyService;
