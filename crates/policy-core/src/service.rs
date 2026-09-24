//! The shared authorization port, implemented by the later policy facade.

use crate::{AuthorizationError, AuthorizationRequest, PolicyDecision};

/// Synchronous, thread-safe authorization independent of agent and engine protocols.
///
/// Implementations evaluate policy only; they must not execute the requested
/// operation. An error is not permission. ApprovalRequired is not approval.
pub trait PolicyService: Send + Sync {
    /// Evaluate one request or return an engine-neutral authorization failure.
    fn authorize(
        &self,
        request: &AuthorizationRequest,
    ) -> Result<PolicyDecision, AuthorizationError>;
}
