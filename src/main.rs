//! Scans Rust project and its dependencies and reports their unsafe code usage.
#![forbid(unsafe_code)]

use std::env::args;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::{error::Error, path::Path};

use cargo_issafe::scan::{Safety, dependency_safety};
use cargo_issafe::tree::{dependency_tree, format_tree};

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
    let manifest = fs::read_to_string("Cargo.toml")?;
    let root_crate = manifest
        .split("[package]")
        .nth(1)
        .and_then(|pkg| {
            pkg.lines()
                .map(str::trim)
                .find(|line| line.starts_with("name = "))
                .and_then(|line| line.split('"').nth(1))
        })
        .ok_or("missing `name` in [package] table")?
        .replace('-', "_");
    let tree = dependency_tree(&root_crate, &lockfile)?;
    print!("{}", format_tree(&tree, &crate_safety));

    if fail_on_unsafe && crate_safety.values().any(|krate| matches!(krate, Safety::UsesUnsafe(_))) {
        return Err("the crate or its dependencies use unsafe code".into());
    }

    Ok(())
}
