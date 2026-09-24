//! The separate semantic boundary between parsing and later policy processing.

use crate::{
    CommandSelector, Effect, NetworkHost, NetworkSelector, ParsedPolicy, PathPattern,
    PolicyDefaults, ResourceSelector, SourceLocation, ValidatedResourceSelector, ValidationError,
    ValidationErrorKind as K,
};
use policy_core::{Action, ResourceKind};
use std::{
    collections::BTreeMap,
    num::NonZeroU16,
    path::{Path, PathBuf},
};

/// An immutable semantically valid rule, not an authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRule {
    id: String,
    description: Option<String>,
    effect: Effect,
    actions: Vec<Action>,
    resource: ValidatedResourceSelector,
}

impl ValidatedRule {
    /// Return the unchanged rule ID.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Return the optional unchanged description.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    /// Return allow, deny, or ask without composing policy layers.
    pub const fn effect(&self) -> Effect {
        self.effect
    }
    /// Return actions in source order, including repeated actions.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
    /// Return the validated selector.
    pub fn resource(&self) -> &ValidatedResourceSelector {
        &self.resource
    }
}

/// An immutable version-one policy that passed semantic validation.
///
/// This establishes language validity only. It is neither normalized against a
/// filesystem nor compiled, matched, composed with other layers, or authorized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPolicy {
    source: PathBuf,
    version: u64,
    defaults: Option<PolicyDefaults>,
    rules: Vec<ValidatedRule>,
    locations: BTreeMap<String, SourceLocation>,
}

impl ValidatedPolicy {
    /// Return the unchanged diagnostic source label.
    pub fn source(&self) -> &Path {
        &self.source
    }
    /// Return the supported version (1).
    pub const fn version(&self) -> u64 {
        self.version
    }
    /// Return explicit defaults, preserving absence without adding fallback.
    pub fn defaults(&self) -> Option<&PolicyDefaults> {
        self.defaults.as_ref()
    }
    /// Return validated rules in source order.
    pub fn rules(&self) -> &[ValidatedRule] {
        &self.rules
    }
    /// Locate a value by the same field paths as `ParsedPolicy::location`.
    pub fn location(&self, field_path: &str) -> Option<SourceLocation> {
        self.locations.get(field_path).copied()
    }
}

/// Validate a parsed policy without I/O, execution, matching, or permission.
///
/// Returns the first semantic error: version first, then each rule in order,
/// checking ID, actions in order, and selector fields. Network host precedes
/// port; command executable precedes arguments in order. Unknown effect names
/// are rejected earlier by `parse_policy`.
pub fn validate_policy(policy: ParsedPolicy) -> Result<ValidatedPolicy, ValidationError> {
    if policy.version() != 1 {
        return Err(ValidationError::new(
            &policy,
            K::UnsupportedVersion,
            "version".into(),
        ));
    }
    let mut identifiers = BTreeMap::new();
    let mut selectors = Vec::with_capacity(policy.rules().len());
    for (index, rule) in policy.rules().iter().enumerate() {
        let prefix = format!("rules[{index}]");
        let id_path = format!("{prefix}.id");
        if rule.id().trim().is_empty() || rule.id().chars().any(char::is_control) {
            return Err(ValidationError::new(&policy, K::InvalidRuleId, id_path));
        }
        if let Some(first) = identifiers.insert(rule.id(), index) {
            return Err(ValidationError::new(&policy, K::DuplicateRuleId, id_path)
                .with_original(policy.location(&format!("rules[{first}].id"))));
        }
        if rule.actions().is_empty() {
            return Err(ValidationError::new(
                &policy,
                K::EmptyActions,
                format!("{prefix}.actions"),
            ));
        }
        let kind = match rule.resource() {
            ResourceSelector::File { .. } => ResourceKind::File,
            ResourceSelector::Command { .. } => ResourceKind::Command,
            ResourceSelector::Git { .. } => ResourceKind::Git,
            ResourceSelector::Network { .. } => ResourceKind::Network,
        };
        for (action_index, action) in rule.actions().iter().enumerate() {
            if action.resource_kind() != kind {
                return Err(ValidationError::new(
                    &policy,
                    K::IncompatibleAction,
                    format!("{prefix}.actions[{action_index}]"),
                ));
            }
        }
        selectors.push(validate_selector(
            &policy,
            rule.resource(),
            &format!("{prefix}.resource"),
        )?);
    }
    let rules = policy
        .rules
        .into_iter()
        .zip(selectors)
        .map(|(rule, resource)| ValidatedRule {
            id: rule.id,
            description: rule.description,
            effect: rule.effect,
            actions: rule.actions,
            resource,
        })
        .collect();
    Ok(ValidatedPolicy {
        source: policy.source,
        version: policy.version,
        defaults: policy.defaults,
        rules,
        locations: policy.locations,
    })
}

fn validate_selector(
    policy: &ParsedPolicy,
    selector: &ResourceSelector,
    prefix: &str,
) -> Result<ValidatedResourceSelector, ValidationError> {
    let error = |kind, field| ValidationError::new(policy, kind, format!("{prefix}.{field}"));
    match selector {
        ResourceSelector::File { pattern } => PathPattern::parse(pattern)
            .map(ValidatedResourceSelector::File)
            .ok_or_else(|| error(K::InvalidPathPattern, "pattern")),
        ResourceSelector::Git { repository } => PathPattern::parse(repository)
            .map(ValidatedResourceSelector::Git)
            .ok_or_else(|| error(K::InvalidPathPattern, "repository")),
        ResourceSelector::Command {
            executable,
            arguments,
        } => {
            if executable.trim().is_empty() || executable.contains('\0') {
                return Err(error(K::InvalidExecutable, "executable"));
            }
            if let Some(arguments) = arguments {
                for (index, argument) in arguments.iter().enumerate() {
                    if argument.contains('\0') {
                        return Err(error(K::InvalidArgument, &format!("arguments[{index}]")));
                    }
                }
            }
            Ok(ValidatedResourceSelector::Command(CommandSelector::new(
                executable.clone(),
                arguments.clone(),
            )))
        }
        ResourceSelector::Network { host, port } => {
            let identity = NetworkHost::parse(host).ok_or_else(|| error(K::InvalidHost, "host"))?;
            let port = port
                .map(|port| NonZeroU16::new(port).ok_or_else(|| error(K::InvalidPort, "port")))
                .transpose()?;
            Ok(ValidatedResourceSelector::Network(NetworkSelector::new(
                host.clone(),
                identity,
                port,
            )))
        }
    }
}
