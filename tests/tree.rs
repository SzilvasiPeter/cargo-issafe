//! Integration tests for the tree module.

#![allow(clippy::unwrap_used)]
use cargo_issafe::error::TreeError;
use cargo_issafe::tree::dependency_tree;

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
