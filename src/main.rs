//! Scans Rust project and its dependencies and reports their unsafe code usage.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::env::args;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::{error::Error, path::Path};

use cargo_issafe::scan::{Safety, dependency_safety};
use cargo_issafe::tree::{DependencyTree, dependency_tree};

const GREEN: &str = "\x1b[32m";
const BLUE: &str = "\x1b[34m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

fn main() -> Result<(), Box<dyn Error>> {
    let target_dir = "target/cargo-issafe";
    let deps = PathBuf::from(target_dir).join("debug/deps");
    let fail_on_unsafe = args().any(|arg| arg == "--fail-on-unsafe");

    // Remove the previous compilation if exists, so the .d files reflect the current dependency graph.
    if Path::new(target_dir).is_dir() {
        fs::remove_dir_all(target_dir)?;
    }

    let check = Command::new("cargo").args(["check", "--target-dir", target_dir]).status()?;
    if !check.success() {
        return Err("cargo check failed".into());
    }

    let crate_safety = dependency_safety(&deps)?;
    let lockfile = fs::read_to_string("Cargo.lock")?;
    let root_crate = env!("CARGO_PKG_NAME").replace('-', "_");
    let tree = dependency_tree(&root_crate, &lockfile)?;
    print_tree(&tree, &crate_safety);

    if fail_on_unsafe && crate_safety.values().any(|krate| matches!(krate, Safety::UsesUnsafe(_))) {
        return Err("the crate or its dependencies use unsafe code".into());
    }

    Ok(())
}

fn print_tree(tree: &DependencyTree, safeties: &HashMap<String, Safety>) {
    print_node(tree, tree.root, safeties, 0);
}

fn print_node(tree: &DependencyTree, idx: usize, safeties: &HashMap<String, Safety>, depth: usize) {
    let node = &tree.nodes[idx];
    let id = &node.id;
    if !safeties.contains_key(id) {
        return;
    }

    let (label, color) = match safeties.get(id) {
        Some(Safety::ForbidsUnsafe) => ("safe".to_string(), GREEN),
        Some(Safety::NoUnsafe) => ("0 unsafe".to_string(), BLUE),
        Some(Safety::UsesUnsafe(count)) => (format!("{count} unsafe"), RED),
        None => unreachable!(),
    };

    let indent = "  ".repeat(depth);
    println!("{indent}- {id} [{color}{label}{RESET}]");

    for &dep_idx in &node.dependencies {
        print_node(tree, dep_idx, safeties, depth + 1);
    }
}
