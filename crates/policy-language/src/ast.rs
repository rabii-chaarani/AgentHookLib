//! Owned syntax types; only the parser constructs aggregates.

use policy_core::Action;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// A one-based line and character column in the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub(crate) line: usize,
    pub(crate) column: usize,
}

impl SourceLocation {
    /// Return the one-based line number.
    pub const fn line(self) -> usize {
        self.line
    }
    /// Return the one-based character column.
    pub const fn column(self) -> usize {
        self.column
    }
}

/// A policy effect, distinct from an authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Permit matching operations, subject to later policy composition.
    Allow,
    /// Deny matching operations.
    Deny,
    /// Require approval for matching operations.
    Ask,
}

/// Optional family defaults, with no inferred fallback.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyDefaults {
    pub(crate) file: Option<Effect>,
    pub(crate) command: Option<Effect>,
    pub(crate) git: Option<Effect>,
    pub(crate) network: Option<Effect>,
}

impl PolicyDefaults {
    /// Return the explicit file default, if present.
    pub const fn file(&self) -> Option<Effect> {
        self.file
    }
    /// Return the explicit command default, if present.
    pub const fn command(&self) -> Option<Effect> {
        self.command
    }
    /// Return the explicit Git default, if present.
    pub const fn git(&self) -> Option<Effect> {
        self.git
    }
    /// Return the explicit network default, if present.
    pub const fn network(&self) -> Option<Effect> {
        self.network
    }
}

/// A resource selector whose strings have not been validated or interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceSelector {
    /// A filesystem selector.
    File {
        /// The uninterpreted path pattern.
        pattern: String,
    },
    /// A command selector.
    Command {
        /// The uninterpreted executable selector.
        executable: String,
        /// Explicit argument selectors, preserving absence and order.
        arguments: Option<Vec<String>>,
    },
    /// A Git repository selector.
    Git {
        /// The uninterpreted repository selector.
        repository: String,
    },
    /// A network destination selector.
    Network {
        /// The uninterpreted host selector.
        host: String,
        /// An explicit port; semantic validation may reject zero.
        port: Option<u16>,
    },
}

/// A structurally parsed rule with no authorization or semantic validity implied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRule {
    pub(crate) id: String,
    pub(crate) description: Option<String>,
    pub(crate) effect: Effect,
    pub(crate) actions: Vec<Action>,
    pub(crate) resource: ResourceSelector,
}

impl PolicyRule {
    /// Return the original rule identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Return the decoded description, if supplied.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    /// Return the explicit effect.
    pub const fn effect(&self) -> Effect {
        self.effect
    }
    /// Return canonical action names in document order.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
    /// Return the unvalidated selector.
    pub fn resource(&self) -> &ResourceSelector {
        &self.resource
    }
}

/// A structurally parsed policy, not a validated policy or permission to execute.
///
/// The version, rule identifiers, action/resource compatibility, and selector
/// grammar still require semantic validation. No layer or fallback is inferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPolicy {
    pub(crate) source: PathBuf,
    pub(crate) version: u64,
    pub(crate) defaults: Option<PolicyDefaults>,
    pub(crate) rules: Vec<PolicyRule>,
    pub(crate) locations: BTreeMap<String, SourceLocation>,
}

impl ParsedPolicy {
    /// Return the diagnostic source label, without resolving it.
    pub fn source(&self) -> &Path {
        &self.source
    }
    /// Return the supplied version without checking support.
    pub const fn version(&self) -> u64 {
        self.version
    }
    /// Return explicit defaults, preserving an absent defaults mapping.
    pub fn defaults(&self) -> Option<&PolicyDefaults> {
        self.defaults.as_ref()
    }
    /// Return rules in document order.
    pub fn rules(&self) -> &[PolicyRule] {
        &self.rules
    }
    /// Locate a value, collection, rule, or action by field path.
    ///
    /// Paths use `$` for the document and otherwise forms such as `version`,
    /// `rules[0]`, `rules[0].resource.pattern`, and `rules[0].actions[1]`.
    /// Missing optional fields have no location. Locations refer to values;
    /// diagnostics for unknown or duplicate fields point to their keys.
    pub fn location(&self, field_path: &str) -> Option<SourceLocation> {
        self.locations.get(field_path).copied()
    }
}
