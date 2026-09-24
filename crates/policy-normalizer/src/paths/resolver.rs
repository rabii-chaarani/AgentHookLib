#![forbid(unsafe_code)]

use super::{
    NamingComponent, NamingSemantics, NormalizationError as E, NormalizedFileResource,
    NormalizedPath, PathIdentity, PathRequirement, error::io_error, platform,
};
use policy_core::{Context, FileAction, FileResource};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

struct Entry {
    directory: bool,
    redirect: Option<PathBuf>,
}

// This deliberately has no mutation, execution, environment, or content-read
// operation. Tests can inject metadata failures without changing process state.
trait ReadOnlyFilesystem {
    fn parse(&self, path: &Path, cwd: &Path) -> Result<(PathBuf, Vec<OsString>), E>;
    fn root(&self, root: &Path) -> Result<PathBuf, E>;
    fn inspect(&self, path: &Path) -> Result<Entry, E>;
    fn naming(&self, directory: &Path) -> Result<NamingSemantics, E>;
    fn stored_name(
        &self,
        parent: &Path,
        name: &OsStr,
        semantics: NamingSemantics,
    ) -> Result<OsString, E>;
}

struct HostFilesystem;
impl ReadOnlyFilesystem for HostFilesystem {
    fn parse(&self, path: &Path, cwd: &Path) -> Result<(PathBuf, Vec<OsString>), E> {
        platform::parse(path, cwd)
    }
    fn root(&self, root: &Path) -> Result<PathBuf, E> {
        fs::canonicalize(root).map_err(io_error)
    }
    fn inspect(&self, path: &Path) -> Result<Entry, E> {
        let metadata = fs::symlink_metadata(path).map_err(io_error)?;
        Ok(Entry {
            directory: metadata.is_dir(),
            redirect: platform::redirect(path, &metadata)?,
        })
    }
    fn naming(&self, directory: &Path) -> Result<NamingSemantics, E> {
        platform::naming(directory)
    }
    fn stored_name(
        &self,
        parent: &Path,
        name: &OsStr,
        semantics: NamingSemantics,
    ) -> Result<OsString, E> {
        platform::stored_name(parent, name, semantics)
    }
}

struct Resolved {
    absolute: PathBuf,
    root: PathBuf,
    components: Vec<NamingComponent>,
    directory: bool,
    exists: bool,
}

impl Resolved {
    fn identity(&self) -> Result<PathIdentity, E> {
        Ok(PathIdentity {
            root: self.root.clone(),
            components: self
                .components
                .iter()
                .map(|part| Ok((part.semantics, part.semantics.name_key(&part.name)?)))
                .collect::<Result<_, E>>()?,
        })
    }
}

fn resolve(
    fs: &impl ReadOnlyFilesystem,
    path: &Path,
    cwd: &Path,
    requirement: PathRequirement,
    depth: usize,
) -> Result<Resolved, E> {
    if depth >= 40 {
        return Err(E::UnresolvedLink);
    }
    let (root, parts) = fs.parse(path, cwd)?;
    let root = fs.root(&root)?;
    let (_, root_parts) = fs.parse(&root, &root)?;
    // A mapped drive root can resolve to a directory beneath another volume's
    // root. Resolve that prefix too so aliases share identity and containment.
    let mut current = if root_parts.iter().any(|part| part != ".") {
        resolve(fs, &root, &root, PathRequirement::Existing, depth + 1)?
    } else {
        Resolved {
            absolute: root.clone(),
            root,
            components: Vec::new(),
            directory: true,
            exists: true,
        }
    };
    // Establish support even for a request naming the filesystem root itself.
    fs.naming(&current.absolute)?;
    for (index, part) in parts.iter().enumerate() {
        if !current.directory {
            return Err(E::NotDirectory);
        }
        if part == "." {
            continue;
        }
        if part == ".." {
            if current.components.pop().is_some() {
                current.absolute.pop();
            }
            continue;
        }
        let semantics = fs.naming(&current.absolute)?;
        semantics.name_key(part)?;
        let candidate = current.absolute.join(part);
        match fs.inspect(&candidate) {
            Ok(entry) => {
                if let Some(target) = entry.redirect {
                    current = resolve(
                        fs,
                        &target,
                        &current.absolute,
                        PathRequirement::Existing,
                        depth + 1,
                    )
                    .map_err(|error| match error {
                        E::MissingTarget | E::MissingParent | E::NotDirectory => E::UnresolvedLink,
                        other => other,
                    })?;
                } else {
                    let name = fs.stored_name(&current.absolute, part, semantics)?;
                    semantics.name_key(&name)?;
                    current.absolute.push(&name);
                    current.components.push(NamingComponent { name, semantics });
                    current.directory = entry.directory;
                }
            }
            Err(E::MissingTarget) if index + 1 < parts.len() => return Err(E::MissingParent),
            Err(E::MissingTarget) if requirement == PathRequirement::AllowMissingLeaf => {
                current.absolute.push(part);
                current.components.push(NamingComponent {
                    name: part.clone(),
                    semantics,
                });
                current.directory = false;
                current.exists = false;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

fn context_directory(fs: &impl ReadOnlyFilesystem, path: &Path) -> Result<Resolved, E> {
    if !path.is_absolute() {
        return Err(E::InvalidContext);
    }
    let resolved =
        resolve(fs, path, path, PathRequirement::Existing, 0).map_err(|error| match error {
            E::UnsupportedNamingSemantics
            | E::UnsupportedPlatform
            | E::UnsupportedWindowsNamespace => error,
            _ => E::InvalidContext,
        })?;
    if !resolved.directory {
        return Err(E::InvalidContext);
    }
    Ok(resolved)
}

fn normalize(
    fs: &impl ReadOnlyFilesystem,
    path: &Path,
    context: &Context,
    requirement: PathRequirement,
) -> Result<NormalizedPath, E> {
    let repository = context_directory(fs, context.repository_root())?;
    let cwd = context_directory(fs, context.working_directory())?;
    let resolved = resolve(fs, path, &cwd.absolute, requirement, 0)?;
    let identity = resolved.identity()?;
    let repository_identity = repository.identity()?;
    let relative = if identity.root == repository_identity.root
        && identity
            .components
            .starts_with(&repository_identity.components)
    {
        let mut relative = PathBuf::new();
        for part in resolved.components.iter().skip(repository.components.len()) {
            relative.push(&part.name);
        }
        Some(if relative.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            relative
        })
    } else {
        None
    };
    Ok(NormalizedPath {
        absolute: resolved.absolute,
        relative,
        exists: resolved.exists,
        components: resolved.components,
        identity,
    })
}

/// Normalize one path using the supplied repository root and operation cwd.
///
/// Errors, unsupported semantics, and outside-root identities never grant
/// permission. Equality uses verified native naming rules; spelling is retained
/// separately. No file is created, modified, or executed.
pub fn normalize_path(
    path: &Path,
    context: &Context,
    requirement: PathRequirement,
) -> Result<NormalizedPath, E> {
    normalize(&HostFilesystem, path, context, requirement)
}

/// Normalize every endpoint for a file operation, returning no partial pair.
///
/// Reads, deletes and rename sources must exist. Writes and rename destinations
/// permit one missing leaf. Final links are always followed to existing targets.
pub fn normalize_file_resource(
    action: FileAction,
    resource: &FileResource,
    context: &Context,
) -> Result<NormalizedFileResource, E> {
    if (action == FileAction::Rename) != resource.destination().is_some() {
        return Err(E::InvalidPath);
    }
    let requirement = if action == FileAction::Write {
        PathRequirement::AllowMissingLeaf
    } else {
        PathRequirement::Existing
    };
    let source = normalize_path(resource.path(), context, requirement)?;
    let destination = resource
        .destination()
        .map(|path| normalize_path(path, context, PathRequirement::AllowMissingLeaf))
        .transpose()?;
    Ok(NormalizedFileResource {
        source,
        destination,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FakeFilesystem {
        inaccessible: bool,
    }
    impl ReadOnlyFilesystem for FakeFilesystem {
        fn parse(&self, path: &Path, _: &Path) -> Result<(PathBuf, Vec<OsString>), E> {
            Ok((
                PathBuf::from("/"),
                path.to_str()
                    .unwrap()
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(OsString::from)
                    .collect(),
            ))
        }
        fn root(&self, path: &Path) -> Result<PathBuf, E> {
            Ok(path.to_owned())
        }
        fn inspect(&self, path: &Path) -> Result<Entry, E> {
            if self.inaccessible {
                return Err(E::Inaccessible);
            }
            Ok(Entry {
                directory: path != Path::new("/sensitive/Mixed/file"),
                redirect: None,
            })
        }
        fn naming(&self, path: &Path) -> Result<NamingSemantics, E> {
            Ok(if path == Path::new("/sensitive/Mixed") {
                NamingSemantics::AsciiInsensitive
            } else {
                NamingSemantics::Exact
            })
        }
        fn stored_name(
            &self,
            _: &Path,
            name: &OsStr,
            semantics: NamingSemantics,
        ) -> Result<OsString, E> {
            semantics.name_key(name)
        }
    }
    #[test]
    fn directory_semantics_are_carried_per_component() {
        let fs = FakeFilesystem {
            inaccessible: false,
        };
        let first = resolve(
            &fs,
            Path::new("/sensitive/Mixed/FILE"),
            Path::new("/"),
            PathRequirement::Existing,
            0,
        )
        .unwrap();
        let second = resolve(
            &fs,
            Path::new("/sensitive/Mixed/file"),
            Path::new("/"),
            PathRequirement::Existing,
            0,
        )
        .unwrap();
        assert_eq!(first.identity().unwrap(), second.identity().unwrap());
        assert_eq!(first.components[1].semantics, NamingSemantics::Exact);
        assert_eq!(
            first.components[2].semantics,
            NamingSemantics::AsciiInsensitive
        );
    }
    #[test]
    fn metadata_errors_never_become_missing_leaves() {
        let fs = FakeFilesystem { inaccessible: true };
        assert!(matches!(
            resolve(
                &fs,
                Path::new("/secret"),
                Path::new("/"),
                PathRequirement::AllowMissingLeaf,
                0
            ),
            Err(E::Inaccessible)
        ));
    }

    struct MappedFilesystem;
    impl ReadOnlyFilesystem for MappedFilesystem {
        fn parse(&self, path: &Path, cwd: &Path) -> Result<(PathBuf, Vec<OsString>), E> {
            if let Some(tail) = path.to_str().unwrap().strip_prefix("/mapped/") {
                return Ok((
                    PathBuf::from("/mapped"),
                    tail.split('/').map(OsString::from).collect(),
                ));
            }
            FakeFilesystem {
                inaccessible: false,
            }
            .parse(path, cwd)
        }
        fn root(&self, path: &Path) -> Result<PathBuf, E> {
            Ok(if path == Path::new("/mapped") {
                PathBuf::from("/actual")
            } else {
                path.to_owned()
            })
        }
        fn inspect(&self, path: &Path) -> Result<Entry, E> {
            if path == Path::new("/unsupported") {
                return Err(E::UnresolvedLink);
            }
            Ok(Entry {
                directory: true,
                redirect: (path == Path::new("/link")).then(|| PathBuf::from("/unsupported")),
            })
        }
        fn naming(&self, _: &Path) -> Result<NamingSemantics, E> {
            Ok(NamingSemantics::Exact)
        }
        fn stored_name(&self, _: &Path, name: &OsStr, _: NamingSemantics) -> Result<OsString, E> {
            Ok(name.to_owned())
        }
    }

    #[test]
    fn mapped_roots_share_the_real_prefix_identity() {
        let mapped = resolve(
            &MappedFilesystem,
            Path::new("/mapped/file"),
            Path::new("/"),
            PathRequirement::Existing,
            0,
        )
        .unwrap();
        let direct = resolve(
            &MappedFilesystem,
            Path::new("/actual/file"),
            Path::new("/"),
            PathRequirement::Existing,
            0,
        )
        .unwrap();
        assert_eq!(mapped.absolute, direct.absolute);
        assert_eq!(mapped.identity().unwrap(), direct.identity().unwrap());
        assert_eq!(mapped.components.len(), 2);
    }

    #[test]
    fn link_targets_are_walked_and_unsupported_intermediate_redirects_rejected() {
        assert!(matches!(
            resolve(
                &MappedFilesystem,
                Path::new("/link"),
                Path::new("/"),
                PathRequirement::Existing,
                0
            ),
            Err(E::UnresolvedLink)
        ));
    }
}
