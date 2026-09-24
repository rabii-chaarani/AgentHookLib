//! Explicit repository and operation context.

use std::path::{Path, PathBuf};

use crate::{
    ValidationError,
    error::{absolute_path, nonblank},
};

/// Context supplied by the caller, without environment or repository discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    repository: String,
    repository_root: PathBuf,
    working_directory: PathBuf,
    branch: Option<String>,
    task: Option<String>,
}

impl Context {
    /// Validate required values and absolute context paths without filesystem I/O.
    ///
    /// `None` means branch/task information is absent, not an inferred default.
    /// Paths need not exist; containment and canonicalization are not checked.
    pub fn new(
        repository: impl Into<String>,
        repository_root: impl Into<PathBuf>,
        working_directory: impl Into<PathBuf>,
        branch: Option<String>,
        task: Option<String>,
    ) -> Result<Self, ValidationError> {
        let repository = repository.into();
        let repository_root = repository_root.into();
        let working_directory = working_directory.into();
        nonblank(&repository, "repository")?;
        absolute_path(&repository_root, "repository_root")?;
        absolute_path(&working_directory, "working_directory")?;
        if let Some(value) = &branch {
            nonblank(value, "branch")?;
        }
        if let Some(value) = &task {
            nonblank(value, "task")?;
        }
        Ok(Self {
            repository,
            repository_root,
            working_directory,
            branch,
            task,
        })
    }

    /// Return the repository identifier.
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Return the supplied repository root.
    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    /// Return the supplied operation working directory.
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    /// Return the explicitly supplied branch, if any.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Return the explicitly supplied task, if any.
    pub fn task(&self) -> Option<&str> {
        self.task.as_deref()
    }
}
