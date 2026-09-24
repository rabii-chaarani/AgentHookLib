//! Native Windows filesystem behavior tests for path normalization.
#![cfg(windows)]

use policy_core::Context;
use policy_normalizer::paths::{
    NamingSemantics, NormalizationError as E, PathRequirement as R, normalize_path,
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    os::windows::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::Command,
};

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    cwd: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let root = temp.path().join("repo");
        let cwd = root.join("src");
        fs::create_dir_all(&cwd).expect("create repository working directory");
        fs::write(cwd.join("file.txt"), b"unchanged").expect("create fixture file");
        Self { temp, root, cwd }
    }

    fn context(&self) -> Context {
        Context::new("repo", &self.root, &self.cwd, None, None).expect("valid fixture context")
    }
}

#[test]
fn local_ntfs_long_case_aliases_keep_stored_spelling_and_identity() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let long_name = format!("LongStoredFilename-{}.txt", "n".repeat(64));
    let stored_path = fixture.cwd.join(&long_name);
    fs::write(&stored_path, b"long name").expect("create long filename");

    let stored = normalize_path(&stored_path, &context, R::Existing)
        .expect("temporary fixture must be on a supported local NTFS volume");
    assert_eq!(
        stored.components().last().unwrap().semantics(),
        NamingSemantics::AsciiInsensitive,
        "the fixture directory must use NTFS's default case-insensitive mode"
    );
    assert_eq!(
        stored.absolute_path().file_name(),
        Some(OsStr::new(&long_name))
    );

    let alias_path = fixture.cwd.join(long_name.to_ascii_uppercase());
    let alias = normalize_path(&alias_path, &context, R::Existing)
        .expect("case alias should resolve to the existing long filename");
    assert_eq!(alias.identity(), stored.identity());
    assert_eq!(alias.absolute_path(), stored.absolute_path());
    assert_eq!(
        alias.absolute_path().file_name(),
        Some(OsStr::new(&long_name))
    );
}

#[test]
fn standard_and_extended_absolute_paths_share_identity() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let standard_path = fixture.cwd.join("file.txt");
    let extended_path = extended_path(&standard_path);

    let standard = normalize_path(&standard_path, &context, R::Existing).unwrap();
    let extended = normalize_path(&extended_path, &context, R::Existing).unwrap();

    assert_eq!(extended.identity(), standard.identity());
    assert_eq!(extended.absolute_path(), standard.absolute_path());
}

#[test]
fn root_relative_paths_use_the_operation_cwds_drive() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let absolute = fixture.cwd.join("file.txt");
    let root_relative = root_relative_path(&absolute);

    let expected = normalize_path(&absolute, &context, R::Existing).unwrap();
    let actual = normalize_path(&root_relative, &context, R::Existing).unwrap();
    assert_eq!(actual.identity(), expected.identity());
    assert_eq!(actual.absolute_path(), expected.absolute_path());
}

#[test]
fn hard_link_names_remain_distinct_identities() {
    let fixture = Fixture::new();
    let original_path = fixture.cwd.join("file.txt");
    let hard_link_path = fixture.cwd.join("hard-link.txt");
    fs::hard_link(&original_path, &hard_link_path).expect("create NTFS hard link");

    let original = normalize_path(&original_path, &fixture.context(), R::Existing).unwrap();
    let hard_link = normalize_path(&hard_link_path, &fixture.context(), R::Existing).unwrap();
    assert_ne!(original.identity(), hard_link.identity());
}

#[test]
fn junction_escape_and_parent_segments_follow_filesystem_order() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let outside = fixture.temp.path().join("outside");
    fs::create_dir_all(&outside).expect("create outside directory");
    fs::write(outside.join("data.txt"), b"outside").expect("create outside file");

    let second = fixture.cwd.join("second-junction");
    let first = fixture.cwd.join("first-junction");
    let escape = fixture.cwd.join("escape-junction");
    create_junction(&second, &outside);
    create_junction(&first, &second);
    create_junction(&escape, &outside);

    let escaped = normalize_path(Path::new("escape-junction/data.txt"), &context, R::Existing)
        .expect("resolve junction to outside file");
    assert!(escaped.is_outside_repository());
    assert_eq!(
        escaped.absolute_path(),
        fs::canonicalize(outside.join("data.txt")).unwrap()
    );

    let chained = normalize_path(Path::new("first-junction/data.txt"), &context, R::Existing)
        .expect("resolve chained junctions");
    assert!(chained.is_outside_repository());
    assert_eq!(chained.absolute_path(), escaped.absolute_path());

    let parent = normalize_path(Path::new("escape-junction/.."), &context, R::Existing)
        .expect("apply parent segment after junction resolution");
    assert!(parent.is_outside_repository());
    assert_eq!(
        parent.absolute_path(),
        fs::canonicalize(fixture.temp.path()).unwrap()
    );
}

#[test]
fn dangling_junction_is_not_a_missing_leaf() {
    let fixture = Fixture::new();
    let target = fixture.temp.path().join("empty-target");
    let junction = fixture.cwd.join("dangling-junction");
    fs::create_dir(&target).expect("create empty junction target");
    create_junction(&junction, &target);
    fs::remove_dir(&target).expect("remove empty junction target");

    assert_eq!(
        normalize_path(
            Path::new("dangling-junction"),
            &fixture.context(),
            R::AllowMissingLeaf
        ),
        Err(E::UnresolvedLink),
    );
}

#[test]
fn ntfs_case_sensitive_directory_uses_exact_per_directory_names() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let case_dir = fixture.cwd.join("case-sensitive");
    fs::create_dir(&case_dir).expect("create empty case-sensitive fixture directory");

    enable_case_sensitive_directory(&case_dir);
    fs::write(case_dir.join("Name.txt"), b"upper spelling").unwrap();
    fs::write(case_dir.join("name.txt"), b"lower spelling").unwrap();

    let upper = normalize_path(Path::new("case-sensitive/Name.txt"), &context, R::Existing)
        .expect("resolve upper-case entry");
    let lower = normalize_path(Path::new("case-sensitive/name.txt"), &context, R::Existing)
        .expect("resolve lower-case entry");

    assert_eq!(
        upper.components().last().unwrap().semantics(),
        NamingSemantics::Exact
    );
    assert_eq!(
        lower.components().last().unwrap().semantics(),
        NamingSemantics::Exact
    );
    assert_ne!(upper.identity(), lower.identity());
    assert_ne!(upper.absolute_path(), lower.absolute_path());
}

fn extended_path(path: &Path) -> PathBuf {
    let mut wide = OsString::from("\\\\?\\");
    wide.push(path.as_os_str());
    PathBuf::from(wide)
}

fn root_relative_path(path: &Path) -> PathBuf {
    let mut components = path.components();
    assert!(matches!(components.next(), Some(Component::Prefix(_))));
    assert!(matches!(components.next(), Some(Component::RootDir)));

    let mut result = PathBuf::from("\\");
    for component in components {
        match component {
            Component::Normal(name) => result.push(name),
            Component::CurDir => {}
            other => panic!("unexpected fixture path component: {other:?}"),
        }
    }
    result
}

fn create_junction(link: &Path, target: &Path) {
    // cmd.exe does not use the C runtime's quoting rules. These are generated
    // fixture paths only; reject shell metacharacters before using raw_arg.
    let fixture_path = |path: &Path| {
        let text = path.to_str().expect("fixture path must be Unicode");
        assert!(
            text.chars()
                .all(|c| c.is_ascii_alphanumeric()
                    || matches!(c, ' ' | '\\' | ':' | '.' | '-' | '_'))
        );
        text.to_owned()
    };
    let command = format!(
        "\"mklink /J \"{}\" \"{}\"\"",
        fixture_path(link),
        fixture_path(target)
    );
    let output = Command::new("cmd.exe")
        .args(["/D", "/S", "/C"])
        .raw_arg(command)
        .output()
        .expect("start cmd.exe to create a temporary junction");
    assert!(
        output.status.success(),
        "mklink /J failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn enable_case_sensitive_directory(directory: &Path) {
    let output = Command::new("fsutil.exe")
        .args(["file", "setCaseSensitiveInfo"])
        .arg(directory)
        .arg("enable")
        .output()
        .expect("start fsutil.exe to configure the temporary directory");
    assert!(
        output.status.success(),
        "could not enable NTFS per-directory case sensitivity for the test fixture; run the Windows test job elevated and ensure this is NTFS: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
