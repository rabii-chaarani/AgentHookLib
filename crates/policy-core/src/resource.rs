//! Owned resource descriptions; no constructors resolve or access resources.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use crate::{
    ResourceKind, ValidationError,
    error::{nonblank, nonempty_path},
};

/// A file path and, for rename, its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResource {
    path: PathBuf,
    destination: Option<PathBuf>,
}

impl FileResource {
    /// Preserve nonempty paths; action-specific rename validation occurs in the request.
    pub fn new(
        path: impl Into<PathBuf>,
        destination: Option<PathBuf>,
    ) -> Result<Self, ValidationError> {
        let path = path.into();
        nonempty_path(&path, "file_path")?;
        if let Some(value) = &destination {
            nonempty_path(value, "rename_destination")?;
        }
        Ok(Self { path, destination })
    }

    /// Return the original file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the original rename destination, if supplied.
    pub fn destination(&self) -> Option<&Path> {
        self.destination.as_deref()
    }
}

/// An executable and argument vector; working directory resides in request context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResource {
    executable: OsString,
    arguments: Vec<OsString>,
}

impl CommandResource {
    /// Preserve an executable and its arguments without shell interpretation.
    /// Empty arguments are valid; only an empty executable is rejected.
    pub fn new(
        executable: impl Into<OsString>,
        arguments: Vec<OsString>,
    ) -> Result<Self, ValidationError> {
        let executable = executable.into();
        if executable.is_empty() {
            return Err(ValidationError::EmptyExecutable);
        }
        Ok(Self {
            executable,
            arguments,
        })
    }

    /// Return the original executable.
    pub fn executable(&self) -> &OsStr {
        &self.executable
    }

    /// Return the original argument vector.
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

/// A repository path for a semantic Git operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitResource {
    repository: PathBuf,
}

impl GitResource {
    /// Preserve a nonempty repository path without discovering or opening it.
    pub fn new(repository: impl Into<PathBuf>) -> Result<Self, ValidationError> {
        let repository = repository.into();
        nonempty_path(&repository, "git_repository")?;
        Ok(Self { repository })
    }

    /// Return the original repository path.
    pub fn repository(&self) -> &Path {
        &self.repository
    }
}

/// A network destination; host interpretation and resolution belong to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkResource {
    host: String,
    port: Option<u16>,
}

impl NetworkResource {
    /// Preserve a nonblank host and optional nonzero port without DNS or I/O.
    pub fn new(host: impl Into<String>, port: Option<u16>) -> Result<Self, ValidationError> {
        let host = host.into();
        nonblank(&host, "network_host")?;
        if port == Some(0) {
            return Err(ValidationError::ZeroPort);
        }
        Ok(Self { host, port })
    }

    /// Return the host exactly as supplied.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Return the supplied port, without a protocol-derived default.
    pub fn port(&self) -> Option<u16> {
        self.port
    }
}

/// A structurally validated resource in one of the initial four families.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    /// A file or rename pair.
    File(FileResource),
    /// A command invocation.
    Command(CommandResource),
    /// A Git repository.
    Git(GitResource),
    /// A network destination.
    Network(NetworkResource),
}

impl Resource {
    /// Return this resource's action family.
    pub const fn kind(&self) -> ResourceKind {
        match self {
            Self::File(_) => ResourceKind::File,
            Self::Command(_) => ResourceKind::Command,
            Self::Git(_) => ResourceKind::Git,
            Self::Network(_) => ResourceKind::Network,
        }
    }
}
