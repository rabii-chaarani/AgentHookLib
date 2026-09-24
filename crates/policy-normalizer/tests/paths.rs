//! Native filesystem contract tests for read-only path normalization.
use policy_core::{Context, FileAction, FileResource};
use policy_normalizer::paths::{
    NamingSemantics, NormalizationError as E, PathRequirement as R, normalize_file_resource,
    normalize_path,
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    cwd: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let cwd = root.join("src");
        fs::create_dir_all(&cwd).unwrap();
        fs::write(cwd.join("file.txt"), b"unchanged").unwrap();
        Self { temp, root, cwd }
    }
    fn context(&self) -> Context {
        Context::new("repo", &self.root, &self.cwd, None, None).unwrap()
    }
}

#[test]
fn equivalent_paths_produce_the_same_identity() {
    let f = Fixture::new();
    let context = f.context();
    let first = normalize_path(Path::new("file.txt"), &context, R::Existing).unwrap();
    for path in [
        f.cwd.join("file.txt"),
        PathBuf::from("./file.txt"),
        PathBuf::from("../src/file.txt"),
        PathBuf::from("../src//./file.txt"),
    ] {
        let other = normalize_path(&path, &context, R::Existing).unwrap();
        assert_eq!(first.identity(), other.identity());
        assert_eq!(first.absolute_path(), other.absolute_path());
    }
    assert_eq!(first.repository_relative(), Some(Path::new("src/file.txt")));
    assert!(first.exists());
}

#[test]
fn normalization_is_idempotent_for_existing_and_missing_targets() {
    let f = Fixture::new();
    for (path, requirement) in [("file.txt", R::Existing), ("new.txt", R::AllowMissingLeaf)] {
        let first = normalize_path(Path::new(path), &f.context(), requirement).unwrap();
        let second = normalize_path(first.absolute_path(), &f.context(), requirement).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.absolute_path(), second.absolute_path());
        assert_eq!(first.components(), second.components());
        assert_eq!(first.exists(), second.exists());
    }
}

#[test]
fn root_and_component_containment_do_not_rebase_outside_paths() {
    let f = Fixture::new();
    let root = normalize_path(Path::new(".."), &f.context(), R::Existing).unwrap();
    assert_eq!(root.repository_relative(), Some(Path::new(".")));
    let outside = normalize_path(Path::new("../.."), &f.context(), R::Existing).unwrap();
    assert!(outside.is_outside_repository());
    assert_eq!(
        outside.absolute_path(),
        fs::canonicalize(f.temp.path()).unwrap()
    );
    let sibling = f.temp.path().join("repo-other");
    fs::create_dir(&sibling).unwrap();
    assert!(
        normalize_path(&sibling, &f.context(), R::Existing)
            .unwrap()
            .is_outside_repository()
    );
    let context = Context::new("repo", &f.root, f.temp.path(), None, None).unwrap();
    assert!(
        normalize_path(Path::new("outside.txt"), &context, R::AllowMissingLeaf)
            .unwrap()
            .is_outside_repository()
    );
}

#[test]
fn missing_leaf_requires_an_existing_parent() {
    let f = Fixture::new();
    let context = f.context();
    let new = normalize_path(Path::new("new.txt"), &context, R::AllowMissingLeaf).unwrap();
    assert!(!new.exists());
    assert!(!new.absolute_path().exists());
    assert_eq!(
        normalize_path(Path::new("new.txt"), &context, R::Existing),
        Err(E::MissingTarget)
    );
    for path in [
        "missing/new.txt",
        "missing/../file.txt",
        "missing/./file.txt",
        "missing/",
        "missing/.",
    ] {
        assert_eq!(
            normalize_path(Path::new(path), &context, R::AllowMissingLeaf),
            Err(E::MissingParent),
            "{path}"
        );
    }
}

#[test]
fn files_cannot_be_traversed_as_directories() {
    let f = Fixture::new();
    for path in [
        "file.txt/child",
        "file.txt/../other",
        "file.txt/",
        "file.txt/.",
    ] {
        assert_eq!(
            normalize_path(Path::new(path), &f.context(), R::AllowMissingLeaf),
            Err(E::NotDirectory),
            "{path}"
        );
    }
}

#[test]
fn context_and_empty_paths_fail_without_fabricated_values() {
    let f = Fixture::new();
    assert_eq!(
        normalize_path(Path::new(""), &f.context(), R::Existing),
        Err(E::InvalidPath)
    );
    for (root, cwd) in [
        (&f.root, f.cwd.join("missing")),
        (&f.root, f.cwd.join("file.txt")),
    ] {
        let context = Context::new("repo", root, cwd, None, None).unwrap();
        assert_eq!(
            normalize_path(Path::new("file.txt"), &context, R::Existing),
            Err(E::InvalidContext)
        );
    }
    let context = Context::new("repo", f.root.join("missing"), &f.cwd, None, None).unwrap();
    assert_eq!(
        normalize_path(Path::new("file.txt"), &context, R::Existing),
        Err(E::InvalidContext)
    );
}

#[test]
fn file_actions_normalize_complete_rename_pairs() {
    let f = Fixture::new();
    let resource = FileResource::new("file.txt", Some("destination.txt".into())).unwrap();
    let pair = normalize_file_resource(FileAction::Rename, &resource, &f.context()).unwrap();
    assert!(pair.path().exists());
    assert!(!pair.destination().unwrap().exists());
    let core = pair.to_file_resource().unwrap();
    assert_eq!(core.path(), pair.path().absolute_path());
    assert_eq!(
        core.destination(),
        Some(pair.destination().unwrap().absolute_path())
    );
    let invalid = FileResource::new("file.txt", Some("missing/destination.txt".into())).unwrap();
    assert_eq!(
        normalize_file_resource(FileAction::Rename, &invalid, &f.context()),
        Err(E::MissingParent)
    );
    assert_eq!(
        normalize_file_resource(FileAction::Read, &resource, &f.context()),
        Err(E::InvalidPath)
    );
    let missing = FileResource::new("new.txt", None).unwrap();
    assert!(normalize_file_resource(FileAction::Write, &missing, &f.context()).is_ok());
    for action in [FileAction::Read, FileAction::Delete] {
        assert_eq!(
            normalize_file_resource(action, &missing, &f.context()),
            Err(E::MissingTarget)
        );
    }
    assert_eq!(
        normalize_file_resource(FileAction::Rename, &missing, &f.context()),
        Err(E::InvalidPath)
    );
}

#[test]
fn naming_keys_are_shared_with_selector_literals_and_fail_closed() {
    assert_eq!(
        NamingSemantics::AsciiInsensitive
            .name_key(OsStr::new("FILE"))
            .unwrap(),
        OsString::from("file")
    );
    assert_ne!(
        NamingSemantics::Exact.name_key(OsStr::new("FILE")).unwrap(),
        NamingSemantics::Exact.name_key(OsStr::new("file")).unwrap()
    );
    for semantics in [
        NamingSemantics::AsciiSensitive,
        NamingSemantics::AsciiInsensitive,
    ] {
        assert_eq!(
            semantics.name_key(OsStr::new("é")),
            Err(E::UnsupportedNamingSemantics)
        );
    }
    for name in ["", ".", "..", "a/b", "a\0b"] {
        assert_eq!(
            NamingSemantics::Exact.name_key(OsStr::new(name)),
            Err(E::InvalidPath)
        );
    }
}

#[test]
fn host_case_rules_apply_to_existing_and_missing_names() {
    let f = Fixture::new();
    let original = normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap();
    let insensitive =
        original.components().last().unwrap().semantics() == NamingSemantics::AsciiInsensitive;
    let alternative = normalize_path(Path::new("FILE.TXT"), &f.context(), R::Existing);
    if insensitive {
        let alternative = alternative.unwrap();
        assert_eq!(alternative.absolute_path(), original.absolute_path());
        assert_eq!(alternative, original);
    } else {
        assert_eq!(alternative, Err(E::MissingTarget));
    }
    let lower = normalize_path(Path::new("new.txt"), &f.context(), R::AllowMissingLeaf).unwrap();
    let upper = normalize_path(Path::new("NEW.TXT"), &f.context(), R::AllowMissingLeaf).unwrap();
    assert_eq!(lower == upper, insensitive);
}

#[test]
fn normalization_preserves_files_contents_permissions_and_modification_times() {
    let f = Fixture::new();
    let file = f.cwd.join("file.txt");
    let before_file = fs::metadata(&file).unwrap();
    let before_directory = fs::metadata(&f.cwd).unwrap();
    let entries_before: Vec<_> = fs::read_dir(&f.cwd)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap();
    normalize_path(Path::new("new.txt"), &f.context(), R::AllowMissingLeaf).unwrap();
    let after_file = fs::metadata(&file).unwrap();
    let after_directory = fs::metadata(&f.cwd).unwrap();
    assert_eq!(fs::read(file).unwrap(), b"unchanged");
    assert_eq!(before_file.permissions(), after_file.permissions());
    assert_eq!(
        before_file.modified().unwrap(),
        after_file.modified().unwrap()
    );
    assert_eq!(
        before_directory.modified().unwrap(),
        after_directory.modified().unwrap()
    );
    assert_eq!(
        before_directory.permissions(),
        after_directory.permissions()
    );
    assert_eq!(
        entries_before,
        fs::read_dir(&f.cwd)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
}

#[test]
fn error_diagnostics_omit_input_paths() {
    let f = Fixture::new();
    let error = normalize_path(
        Path::new("private-secret/missing"),
        &f.context(),
        R::Existing,
    )
    .unwrap_err();
    let standard: &dyn std::error::Error = &error;
    assert!(standard.source().is_none());
    for diagnostic in [format!("{error}"), format!("{error:?}")] {
        assert!(!diagnostic.contains("private-secret"));
        assert!(!diagnostic.contains(f.temp.path().to_str().unwrap()));
    }
}

#[test]
fn normalization_output_child() {
    let Some(root) = std::env::var_os("POLICY_NORMALIZER_OUTPUT_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let context = Context::new("repo", &root, root.join("src"), None, None).unwrap();
    normalize_path(Path::new("file.txt"), &context, R::Existing).unwrap();
    normalize_path(Path::new("new.txt"), &context, R::AllowMissingLeaf).unwrap();
    assert!(normalize_path(Path::new("private-secret/missing"), &context, R::Existing).is_err());
}

#[test]
fn normalization_does_not_emit_protocol_or_diagnostic_output() {
    let f = Fixture::new();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "normalization_output_child",
            "--nocapture",
            "--quiet",
        ])
        .env("POLICY_NORMALIZER_OUTPUT_FIXTURE", &f.root)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for line in stdout.lines().filter(|line| !line.is_empty()) {
        assert!(
            line == "running 1 test"
                || line == "."
                || line.starts_with("test result: ok. 1 passed;"),
            "unexpected stdout: {line}"
        );
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};

    #[test]
    fn symlink_targets_and_parent_segments_use_filesystem_order() {
        let f = Fixture::new();
        fs::create_dir(f.temp.path().join("outside")).unwrap();
        fs::write(f.temp.path().join("outside/data"), b"outside").unwrap();
        symlink(f.temp.path().join("outside"), f.cwd.join("escape")).unwrap();
        symlink(&f.cwd, f.root.join("alias")).unwrap();
        let alias =
            normalize_path(Path::new("../alias/file.txt"), &f.context(), R::Existing).unwrap();
        let direct = normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap();
        assert_eq!(alias, direct);
        let escape = normalize_path(Path::new("escape/data"), &f.context(), R::Existing).unwrap();
        assert!(escape.is_outside_repository());
        let parent = normalize_path(Path::new("escape/.."), &f.context(), R::Existing).unwrap();
        assert_eq!(
            parent.absolute_path(),
            fs::canonicalize(f.temp.path()).unwrap()
        );
        assert!(parent.is_outside_repository());
        let new =
            normalize_path(Path::new("escape/new"), &f.context(), R::AllowMissingLeaf).unwrap();
        assert!(!new.exists());
        assert!(new.is_outside_repository());
    }

    #[test]
    fn final_links_identify_referents_and_dangling_links_never_become_new_files() {
        let f = Fixture::new();
        symlink("file.txt", f.cwd.join("alias")).unwrap();
        let original = normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap();
        assert_eq!(
            normalize_path(Path::new("alias"), &f.context(), R::Existing).unwrap(),
            original
        );
        let resource = FileResource::new("alias", None).unwrap();
        assert_eq!(
            normalize_file_resource(FileAction::Delete, &resource, &f.context())
                .unwrap()
                .path(),
            &original
        );
        symlink("missing", f.cwd.join("dangling")).unwrap();
        symlink("loop", f.cwd.join("loop")).unwrap();
        for path in ["dangling", "loop"] {
            assert_eq!(
                normalize_path(Path::new(path), &f.context(), R::AllowMissingLeaf),
                Err(E::UnresolvedLink)
            );
        }
    }

    #[test]
    fn symlinked_root_and_cwd_are_established_without_rebasing() {
        let f = Fixture::new();
        let alias = f.temp.path().join("alias");
        symlink(&f.root, &alias).unwrap();
        let context = Context::new("repo", &alias, alias.join("src"), None, None).unwrap();
        assert_eq!(
            normalize_path(Path::new("file.txt"), &context, R::Existing).unwrap(),
            normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap()
        );
    }

    #[test]
    fn hard_link_names_remain_distinct() {
        let f = Fixture::new();
        fs::hard_link(f.cwd.join("file.txt"), f.cwd.join("hard.txt")).unwrap();
        assert_ne!(
            normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap(),
            normalize_path(Path::new("hard.txt"), &f.context(), R::Existing).unwrap()
        );
    }

    #[test]
    fn native_names_are_never_converted_lossily() {
        let f = Fixture::new();
        let name = OsString::from_vec(b"native-\xff".to_vec());
        let known = normalize_path(Path::new("file.txt"), &f.context(), R::Existing).unwrap();
        if known.components().last().unwrap().semantics() == NamingSemantics::Exact {
            fs::write(f.cwd.join(&name), b"native").unwrap();
            let result = normalize_path(Path::new(&name), &f.context(), R::Existing);
            assert_eq!(
                result.unwrap().absolute_path().file_name(),
                Some(name.as_os_str())
            );
        } else {
            assert_eq!(
                normalize_path(Path::new(&name), &f.context(), R::AllowMissingLeaf),
                Err(E::UnsupportedNamingSemantics)
            );
        }
    }
}
