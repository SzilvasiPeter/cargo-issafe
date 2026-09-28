//! Scans Rust project dependencies and reports their unsafe code usage.
#![forbid(unsafe_code)]

use std::env::args;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::{error::Error, path::Path};

use cargo_issafe::scan::{Safety, dependency_safety};

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

    let mut has_unsafe = false;
    for (crate_name, safety) in dependency_safety(&deps)? {
        let safety = match safety {
            Safety::ForbidsUnsafe => "safe".to_string(),
            Safety::NoUnsafe => "no unsafe usage".to_string(),
            Safety::UsesUnsafe(count) => {
                has_unsafe = true;
                format!("unsafe ({count})")
            }
        };
        println!("{crate_name}: {safety}");
    }

    if fail_on_unsafe && has_unsafe {
        return Err("dependencies use unsafe code".into());
    }

    Ok(())
}
