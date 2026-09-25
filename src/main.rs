//! Am I Safe?
#![forbid(unsafe_code)]

use std::fs;
use std::process::Command;
use std::{error::Error, path::Path};

use cargo_issafe::scan::{Safety, dependency_safety};

// TODO: make the binary cargo plugin compatible
fn main() -> Result<(), Box<dyn Error>> {
    let target_dir = "target/cargo-issafe";
    let deps = format!("{target_dir}/debug/deps");

    // Remove the previous compilation if exists, so the .d files reflect the current dependency graph.
    if Path::new(target_dir).is_dir() {
        fs::remove_dir_all(target_dir)?;
    }

    let check = Command::new("cargo").args(["check", "--target-dir", target_dir]).status()?;
    if !check.success() {
        return Err("cargo check failed".into());
    }

    for (name, safety) in dependency_safety(&deps)? {
        let flag = match safety {
            Safety::ForbidsUnsafe => "safe",
            Safety::NoUnsafe => "no unsafe usage",
            Safety::UsesUnsafe => "unsafe",
        };
        println!("{name}: {flag}");
    }

    Ok(())
}
