//! Immutable principal/action/resource/context requests.

use crate::{Action, Context, FileAction, Principal, Resource, ValidationError};

/// A structurally valid request, not an authorization or normalization guarantee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    principal: Principal,
    action: Action,
    resource: Resource,
    context: Context,
}

impl AuthorizationRequest {
    /// Validate resource compatibility and rename shape without evaluating policy.
    pub fn new(
        principal: Principal,
        action: Action,
        resource: Resource,
        context: Context,
    ) -> Result<Self, ValidationError> {
        if action.resource_kind() != resource.kind() {
            return Err(ValidationError::IncompatibleActionResource {
                action,
                resource: resource.kind(),
            });
        }
        if let Resource::File(file) = &resource {
            match (action, file.destination().is_some()) {
                (Action::File(FileAction::Rename), false) => {
                    return Err(ValidationError::MissingRenameDestination);
                }
                (Action::File(FileAction::Rename), true) => {}
                (_, true) => return Err(ValidationError::UnexpectedRenameDestination),
                (_, false) => {}
            }
        }
        Ok(Self {
            principal,
            action,
            resource,
            context,
        })
    }

    /// Return the requesting identity.
    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    /// Return the requested action.
    pub fn action(&self) -> Action {
        self.action
    }

    /// Return immutable access to the resource description.
    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    /// Return immutable access to operation context.
    pub fn context(&self) -> &Context {
        &self.context
    }
}
