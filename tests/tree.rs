//! Integration tests for the tree module.

#![allow(clippy::unwrap_used)]
use std::collections::HashMap;

use cargo_issafe::error::TreeError;
use cargo_issafe::scan::Safety;
use cargo_issafe::tree::{dependency_tree, format_tree};

#[test]
fn cargo_issafe_lockfile() {
    let input = r#"
        [[package]]
        name = "cargo-issafe"
        version = "0.3.0"
        dependencies = [
         "rustc_lexer",
        ]

        [[package]]
        name = "rustc_lexer"
        version = "0.1.0"
        dependencies = [
         "unicode-xid",
        ]

        [[package]]
        name = "unicode-xid"
        version = "0.2.6"
        "#;

    let tree = dependency_tree("cargo_issafe", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.root, 0);
        assert_eq!(tree.nodes[0].id, "cargo_issafe");
        assert_eq!(tree.nodes[0].dependencies, [1]);
        assert_eq!(tree.nodes[1].id, "rustc_lexer-0.1.0");
        assert_eq!(tree.nodes[1].dependencies, [2]);
        assert_eq!(tree.nodes[2].id, "unicode_xid-0.2.6");
        assert_eq!(tree.nodes[2].dependencies, []);
    }
}

#[test]
fn versioned_dependencies() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "syn 2.0.119",
         "syn 3.0.6",
        ]

        [[package]]
        name = "syn"
        version = "2.0.119"

        [[package]]
        name = "syn"
        version = "0.1.0"

        [[package]]
        name = "syn"
        version = "3.0.6"
        "#;

    let tree = dependency_tree("root_crate", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.nodes[0].dependencies, [1, 3]);
    }
}

#[test]
fn unversioned_dependencies() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "aaa",
         "bbb",
        ]

        [[package]]
        name = "aaa"
        version = "1.0.0"
        dependencies = [
         "bbb",
        ]

        [[package]]
        name = "bbb"
        version = "2.0.0"
        "#;

    let tree = dependency_tree("root_crate", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.root, 0);
        assert_eq!(tree.nodes[0].dependencies, [1, 2]);
        assert_eq!(tree.nodes[1].dependencies, [2]);
        assert_eq!(tree.nodes[2].dependencies, []);
    }
}

#[test]
fn root_is_not_first() {
    let input = r#"
        [[package]]
        name = "aaa"
        version = "1.0.0"

        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "aaa",
         "zzz",
        ]

        [[package]]
        name = "zzz"
        version = "2.0.0"
        "#;

    let tree = dependency_tree("root_crate", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.root, 1);
        assert_eq!(tree.nodes[tree.root].id, "root_crate");
        assert_eq!(tree.nodes[0].dependencies, []);
        assert_eq!(tree.nodes[1].dependencies, [0, 2]);
        assert_eq!(tree.nodes[2].dependencies, []);
    }
}

#[test]
fn ambiguous_dependency_resolves_to_first() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "syn",
        ]

        [[package]]
        name = "syn"
        version = "2.0.119"

        [[package]]
        name = "syn"
        version = "3.0.6"
        "#;

    let tree = dependency_tree("root_crate", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.nodes[0].dependencies, [1]);
    }
}

#[test]
fn unresolved_dependency_errors() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "nonexistent",
        ]
        "#;

    let err = dependency_tree("root_crate", input).unwrap_err();
    assert_eq!(err, TreeError::UnresolvedDependency("nonexistent".to_string()));
    assert_eq!(err.to_string(), "`nonexistent` is missing in the lockfile");
}

#[test]
fn unknown_root_errors() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        "#;

    let err = dependency_tree("nonexistent", input).unwrap_err();
    assert_eq!(err, TreeError::UnresolvedDependency("nonexistent".to_string()));
    assert_eq!(err.to_string(), "`nonexistent` is missing in the lockfile");
}

#[test]
fn empty_lockfile_errors() {
    let err = dependency_tree("root_crate", "version = 4\n").unwrap_err();
    assert_eq!(err, TreeError::Empty);
    assert_eq!(err.to_string(), "lockfile contains no packages");
}

#[test]
fn unescaped_name_field_errors() {
    let input = r#"
        [[package]]
        name = "root_crate"
        version = "0.1.0"

        [[package]]
        name = unescaped
        version = "0.1.0"
        "#;

    let err = dependency_tree("root_crate", input).unwrap_err();
    assert_eq!(err, TreeError::MissingField);
    assert_eq!(err.to_string(), "package block is missing `name` or `version`");
}

#[test]
fn empty_name_field_errors() {
    let input = r#"
        [[package]]
        name = "root_crate"
        version = "0.1.0"

        [[package]]
        name = ""
        version = "0.1.0"
        "#;

    let err = dependency_tree("root_crate", input).unwrap_err();
    assert_eq!(err, TreeError::MissingField);
    assert_eq!(err.to_string(), "package block is missing `name` or `version`");
}

#[test]
fn missing_version_field_errors() {
    let input = r#"
        [[package]]
        name = "root-crate"
        "#;

    let err = dependency_tree("root_crate", input).unwrap_err();
    assert_eq!(err, TreeError::MissingField);
    assert_eq!(err.to_string(), "package block is missing `name` or `version`");
}

#[test]
fn format_tree_collapses_repeated_dependencies() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "aaa",
         "bbb",
        ]

        [[package]]
        name = "aaa"
        version = "1.0.0"
        dependencies = [
         "bbb",
        ]

        [[package]]
        name = "bbb"
        version = "2.0.0"
        dependencies = [
         "ccc",
        ]

        [[package]]
        name = "ccc"
        version = "3.0.0"
        "#;

    let tree = dependency_tree("root_crate", input).unwrap();

    let mut safeties = HashMap::new();
    safeties.insert("root_crate".to_string(), Safety::NoUnsafe);
    safeties.insert("aaa-1.0.0".to_string(), Safety::ForbidsUnsafe);
    safeties.insert("bbb-2.0.0".to_string(), Safety::UsesUnsafe(1));
    safeties.insert("ccc-3.0.0".to_string(), Safety::NoUnsafe);

    let output = format_tree(&tree, &safeties);

    let expected = "\
- root_crate \x1b[34m[0 unsafe]\x1b[0m
  - aaa-1.0.0 \x1b[32m[safe]\x1b[0m
    - bbb-2.0.0 \x1b[31m[1 unsafe]\x1b[0m
      - ccc-3.0.0 \x1b[34m[0 unsafe]\x1b[0m
  - bbb-2.0.0 \x1b[31m[1 unsafe]\x1b[0m
";
    assert_eq!(output, expected);
}

#[test]
fn format_tree_falls_back_to_versionless_safety_key() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "path-dep",
        ]

        [[package]]
        name = "path-dep"
        version = "0.1.0"
        "#;

    let tree = dependency_tree("root_crate", input).unwrap();

    // Local crates have no version in the safety map, see `extract_version`.
    let mut safeties = HashMap::new();
    safeties.insert("root_crate".to_string(), Safety::NoUnsafe);
    safeties.insert("path_dep".to_string(), Safety::ForbidsUnsafe);

    let output = format_tree(&tree, &safeties);

    let expected = "\
- root_crate \x1b[34m[0 unsafe]\x1b[0m
  - path_dep-0.1.0 \x1b[32m[safe]\x1b[0m
";
    assert_eq!(output, expected);
}

#[test]
fn format_tree_skips_package_without_safety_entry() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        dependencies = [
         "path-dep",
        ]

        [[package]]
        name = "path-dep"
        version = "0.1.0"
        "#;

    let tree = dependency_tree("root_crate", input).unwrap();

    let mut safeties = HashMap::new();
    safeties.insert("root_crate".to_string(), Safety::NoUnsafe);

    let output = format_tree(&tree, &safeties);

    let expected = "- root_crate \x1b[34m[0 unsafe]\x1b[0m\n";
    assert_eq!(output, expected);
}
