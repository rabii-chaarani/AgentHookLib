//! Policy outcomes and source-based explanations, independent of any evaluator.

use std::path::{Path, PathBuf};

use crate::{
    Action, ValidationError,
    error::{nonblank, nonempty_path},
};

/// An authorization outcome. Approval required is not permission to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionKind {
    /// The requested operation is permitted by policy.
    Allow,
    /// The requested operation is denied by policy.
    Deny,
    /// The operation needs a native user approval decision.
    ApprovalRequired,
}

/// Policy layers supported by the first release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyLayer {
    /// Repository-wide policy.
    Repository,
    /// Optional task-specific policy.
    Task,
}

/// The originating policy layer and document path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySource {
    layer: PolicyLayer,
    path: PathBuf,
}

impl PolicySource {
    /// Record a nonempty source path without loading or resolving it.
    pub fn new(layer: PolicyLayer, path: impl Into<PathBuf>) -> Result<Self, ValidationError> {
        let path = path.into();
        nonempty_path(&path, "policy_source")?;
        Ok(Self { layer, path })
    }

    /// Return the originating policy layer.
    pub fn layer(&self) -> PolicyLayer {
        self.layer
    }

    /// Return the source document identifier as a path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A rule identity qualified by its source to avoid cross-layer ambiguity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReference {
    source: PolicySource,
    rule_id: String,
}

impl RuleReference {
    /// Record a nonblank rule identifier without rewriting it.
    pub fn new(source: PolicySource, rule_id: impl Into<String>) -> Result<Self, ValidationError> {
        let rule_id = rule_id.into();
        nonblank(&rule_id, "rule_id")?;
        Ok(Self { source, rule_id })
    }

    /// Return the originating policy document and layer.
    pub fn source(&self) -> &PolicySource {
        &self.source
    }

    /// Return the source rule's identifier.
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }
}

/// Evidence explaining a decision without inventing identifiers for defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionReason {
    /// An explicit rule contributed to the decision.
    MatchedRule(RuleReference),
    /// A layer's action default contributed to the decision.
    ActionDefault {
        /// The policy document containing the default.
        source: PolicySource,
        /// The action whose default applied.
        action: Action,
    },
    /// No applicable permission was found; this is not an explicit rule ID.
    NoApplicablePermission,
}

/// A policy decision and its explanations; evaluation failures use a separate error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    kind: DecisionKind,
    reasons: Vec<DecisionReason>,
}

impl PolicyDecision {
    /// Preserve a decision and at least one explanation.
    /// The evaluator remains responsible for the semantic correctness of the evidence.
    pub fn new(kind: DecisionKind, reasons: Vec<DecisionReason>) -> Result<Self, ValidationError> {
        if reasons.is_empty() {
            return Err(ValidationError::MissingExplanation);
        }
        Ok(Self { kind, reasons })
    }

    /// Return the outcome without collapsing approval into allow or deny.
    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    /// Return explanations in the order supplied by the evaluator.
    pub fn reasons(&self) -> &[DecisionReason] {
        &self.reasons
    }
}
