//! Integration tests for the tree module.

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

    let tree = dependency_tree("cargo-issafe", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.root, 0);
        assert_eq!(tree.nodes[0].name, "cargo-issafe");
        assert_eq!(tree.nodes[0].dependencies, [1]);
        assert_eq!(tree.nodes[1].dependencies, [2]);
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
        version = "3.0.6"
        "#;

    let tree = dependency_tree("root-crate", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        // Versioned entries must resolve to the matching `syn`, not the
        // first one in the file.
        assert_eq!(tree.nodes[0].dependencies, [1, 2]);
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

    let tree = dependency_tree("root-crate", input);
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

    let tree = dependency_tree("root-crate", input);
    assert!(tree.is_ok());
    if let Ok(tree) = tree {
        assert_eq!(tree.root, 1);
        assert_eq!(tree.nodes[tree.root].name, "root-crate");
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

    let tree = dependency_tree("root-crate", input);
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

    assert_eq!(
        dependency_tree("root-crate", input),
        Err(TreeError::UnresolvedDependency("nonexistent".to_string()))
    );
}

#[test]
fn unknown_root_errors() {
    let input = r#"
        [[package]]
        name = "root-crate"
        version = "0.1.0"
        "#;

    assert_eq!(
        dependency_tree("nonexistent", input),
        Err(TreeError::UnresolvedDependency("nonexistent".to_string()))
    );
}

#[test]
fn empty_lockfile_errors() {
    assert_eq!(dependency_tree("root-crate", "version = 4\n"), Err(TreeError::Empty));
}

#[test]
fn missing_field_errors() {
    let input = r#"
        [[package]]
        name = "root-crate"
        "#;

    assert_eq!(dependency_tree("root-crate", input), Err(TreeError::MissingField));
}
