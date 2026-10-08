//! Scans Rust project and its dependencies and reports their unsafe code usage.
#![forbid(unsafe_code)]

use std::env::args;
use std::error::Error;
use std::fs;
use std::process::Command;

use cargo_issafe::manifest::package_name;
use cargo_issafe::scan::{Safety, compiled_dep_files, dependency_safety};
use cargo_issafe::tree::{dependency_tree, format_tree};

fn main() -> Result<(), Box<dyn Error>> {
    let fail_on_unsafe = args().any(|arg| arg == "--fail-on-unsafe");

    let check = Command::new("cargo").args(["check", "--message-format=json"]).output()?;
    if !check.status.success() {
        return Err("cargo check failed".into());
    }
    let stdout = String::from_utf8_lossy(&check.stdout);
    let dep_files = compiled_dep_files(&stdout);

    let crate_safety = dependency_safety(&dep_files)?;
    let lockfile = fs::read_to_string("Cargo.lock").map_err(|err| format!("Cargo.lock: {err}"))?;
    let manifest = fs::read_to_string("Cargo.toml").map_err(|err| format!("Cargo.toml: {err}"))?;
    let root_crate = package_name(&manifest).ok_or("missing `name` in [package] table")?;
    let normalized_root = root_crate.replace('-', "_");
    let tree = dependency_tree(&normalized_root, &lockfile)?;
    print!("{}", format_tree(&tree, &crate_safety));

    if fail_on_unsafe && crate_safety.values().any(|krate| matches!(krate, Safety::UsesUnsafe(_))) {
        return Err("the crate or its dependencies use unsafe code".into());
    }

    Ok(())
}
