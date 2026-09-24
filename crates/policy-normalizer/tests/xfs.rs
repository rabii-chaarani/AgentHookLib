//! Native XFS mode regression, run explicitly against CI's disposable mounts.
#![cfg(target_os = "linux")]

use policy_core::Context;
use policy_normalizer::paths::{NamingSemantics, PathRequirement, normalize_path};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "requires the two XFS mounts provisioned by the XFS CI job"]
fn native_xfs_modes_preserve_equivalence_and_stored_spelling() {
    let fixtures = PathBuf::from(
        std::env::var_os("POLICY_NORMALIZER_XFS_ROOT").expect("XFS fixture root must be supplied"),
    );
    for (directory, semantics) in [
        ("exact", NamingSemantics::Exact),
        ("insensitive", NamingSemantics::AsciiInsensitive),
    ] {
        let root = fixtures.join(directory);
        let context = Context::new("xfs-test", &root, &root, None, None).unwrap();
        fs::write(root.join("StoredName"), b"unchanged").unwrap();
        let before = fs::metadata(&root).unwrap().modified().unwrap();
        let original =
            normalize_path(Path::new("StoredName"), &context, PathRequirement::Existing).unwrap();
        assert_eq!(original.components().last().unwrap().semantics(), semantics);
        let alias = normalize_path(
            Path::new("STOREDNAME"),
            &context,
            PathRequirement::AllowMissingLeaf,
        )
        .unwrap();
        if semantics == NamingSemantics::AsciiInsensitive {
            assert_eq!(original, alias);
            assert!(alias.exists());
            assert_eq!(alias.absolute_path().file_name().unwrap(), "StoredName");
        } else {
            assert_ne!(original, alias);
            assert!(!alias.exists());
        }
        let new_lower = normalize_path(
            Path::new("new"),
            &context,
            PathRequirement::AllowMissingLeaf,
        )
        .unwrap();
        let new_upper = normalize_path(
            Path::new("NEW"),
            &context,
            PathRequirement::AllowMissingLeaf,
        )
        .unwrap();
        assert_eq!(
            new_lower == new_upper,
            semantics == NamingSemantics::AsciiInsensitive
        );
        assert!(!root.join("new").exists());
        assert!(!root.join("NEW").exists());
        assert_eq!(fs::read(root.join("StoredName")).unwrap(), b"unchanged");
        assert_eq!(fs::metadata(&root).unwrap().modified().unwrap(), before);
    }
}
