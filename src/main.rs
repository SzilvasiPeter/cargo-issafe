//! Am I Safe?
#![forbid(unsafe_code)]

use std::error::Error;
use std::fs;
use std::process::Command;

use cargo_issafe::{Unsafe, scan_unsafe};

fn main() -> Result<(), Box<dyn Error>> {
    let target_dir = "target/cargo-issafe";
    let deps = format!("{target_dir}/debug/deps");

    // Remove the previous build so the .d files reflect only the current dependency graph.
    if let Err(err) = fs::remove_dir_all(target_dir) {
        println!("failed to remove {target_dir}: {err}");
    }

    let check = Command::new("cargo").args(["check", "--target-dir", target_dir]).status()?;
    if !check.success() {
        return Err("cargo check failed".into());
    }

    for (name, safety) in scan_unsafe(&deps)? {
        let flag = match safety {
            Unsafe::Forbidden => "safe",
            Unsafe::Absent => "no unsafe usage",
            Unsafe::Present => "unsafe",
        };
        println!("{name}: {flag}");
    }

    Ok(())
}
