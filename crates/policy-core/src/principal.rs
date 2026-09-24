//! Coding-agent identities.

use crate::{ValidationError, error::nonblank};

/// The coding-agent runtime requesting authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentKind {
    /// The Codex runtime.
    Codex,
    /// The Claude runtime.
    Claude,
}

/// An agent and its explicitly supplied session identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    kind: AgentKind,
    session_id: String,
}

impl Principal {
    /// Construct a principal without trimming or synthesizing its session ID.
    pub fn new(kind: AgentKind, session_id: impl Into<String>) -> Result<Self, ValidationError> {
        let session_id = session_id.into();
        nonblank(&session_id, "session_id")?;
        Ok(Self { kind, session_id })
    }

    /// Return the agent kind.
    pub fn kind(&self) -> AgentKind {
        self.kind
    }

    /// Return the session identifier exactly as supplied.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}
