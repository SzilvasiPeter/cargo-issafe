//! Scans Rust project and its dependencies and reports their unsafe code usage.
#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::env::args;
use std::error::Error;
use std::fs;
use std::process::Command;

use cargo_issafe::manifest::{manifest_paths, package_name};
use cargo_issafe::scan::{Safety, compiled_units, dependency_safety};
use cargo_issafe::tree::{dependency_tree, format_tree};

const HELP: &str = "cargo-issafe - detect `unsafe` keyword usage in crates and dependencies

Usage: cargo issafe [--fail-on-unsafe] [--workspace]

Options:
    --fail-on-unsafe    fail when the crate or its dependencies use unsafe code
    --workspace         scan all workspace members
    --help              print this help message
";

fn main() -> Result<(), Box<dyn Error>> {
    if args().any(|arg| arg == "--help") {
        print!("{HELP}");
        return Ok(());
    }

    let fail_on_unsafe = args().any(|arg| arg == "--fail-on-unsafe");
    let workspace = args().find(|arg| arg == "--workspace");

    let check_args = ["check", "--message-format=json"];
    let check = Command::new("cargo").args(check_args).args(&workspace).output()?;
    if !check.status.success() {
        return Err("the `cargo check` command failed.".into());
    }

    let metadata_args = ["metadata", "--no-deps", "--format-version=1"];
    let metadata = Command::new("cargo").args(metadata_args).output()?;
    if !metadata.status.success() {
        return Err("the `cargo metadata` command failed.".into());
    }

    let mut roots = Vec::new();
    let metadata_stdout = String::from_utf8_lossy(&metadata.stdout);
    for path in manifest_paths(&metadata_stdout) {
        let manifest = fs::read_to_string(path).map_err(|err| format!("{path}: {err}"))?;
        if let Some(name) = package_name(&manifest) {
            roots.push(name.to_string());
        }
    }

    if roots.len() > 1 && workspace.is_none() {
        return Err("cargo workspace is detected, use the `--workspace` argument.".into());
    }

    let lockfile = fs::read_to_string("Cargo.lock").map_err(|err| format!("Cargo.lock: {err}"))?;
    let stdout = String::from_utf8_lossy(&check.stdout);
    let dep_units = compiled_units(&stdout);
    let crate_safety = dependency_safety(&dep_units)?;
    let mut visited = HashSet::new();
    for root in roots {
        let tree = dependency_tree(&root, &lockfile)?;
        print!("{}", format_tree(&tree, &crate_safety, &mut visited));
        println!("---");
    }

    if fail_on_unsafe && crate_safety.values().any(|krate| matches!(krate, Safety::UsesUnsafe(_))) {
        return Err("the crate or its dependencies use unsafe code.".into());
    }

    Ok(())
}
