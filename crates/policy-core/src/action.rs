//! Action families and their compatible resource kinds.

/// The four resource families supported by the initial policy model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// A filesystem entry.
    File,
    /// An executable and its arguments.
    Command,
    /// A Git repository.
    Git,
    /// A network destination.
    Network,
}

/// Operations on filesystem entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileAction {
    /// Read an entry.
    Read,
    /// Write an entry.
    Write,
    /// Delete an entry.
    Delete,
    /// Rename an entry to a supplied destination.
    Rename,
}

/// Operations on commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandAction {
    /// Execute a command.
    Execute,
}

/// Semantic operations on Git repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitAction {
    /// Create a commit.
    Commit,
    /// Check out a revision.
    Checkout,
    /// Reset without the hard-reset classification.
    Reset,
    /// Push without the force-push classification.
    Push,
    /// Perform a hard reset.
    ResetHard,
    /// Perform a force push.
    ForcePush,
}

/// Operations on network destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkAction {
    /// Connect to a destination.
    Connect,
}

/// An agent-neutral action with an explicit resource family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// A file operation.
    File(FileAction),
    /// A command operation.
    Command(CommandAction),
    /// A repository operation.
    Git(GitAction),
    /// A network operation.
    Network(NetworkAction),
}

impl Action {
    /// Return the resource family accepted by this action.
    pub const fn resource_kind(self) -> ResourceKind {
        match self {
            Self::File(_) => ResourceKind::File,
            Self::Command(_) => ResourceKind::Command,
            Self::Git(_) => ResourceKind::Git,
            Self::Network(_) => ResourceKind::Network,
        }
    }
}
