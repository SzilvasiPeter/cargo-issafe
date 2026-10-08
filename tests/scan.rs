//! Integration tests for the scan module.

#![allow(clippy::unwrap_used)]
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use cargo_issafe::error::ScanError;
use cargo_issafe::scan::{Safety, dependency_safety};

fn create_test_dir() -> PathBuf {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = env::temp_dir().join(format!("cargo-issafe-tests-{}-{timestamp}", process::id()));
    let create_result = fs::create_dir(&path);
    assert!(create_result.is_ok());
    path
}

fn write_dependency(deps: &Path, dep_file_name: &str, sources: &[(&str, &str)]) -> PathBuf {
    let mut dep_info = String::new();
    for &(source_name, source) in sources {
        let source_path = deps.join(source_name);
        let write_result = fs::write(&source_path, source);
        assert!(write_result.is_ok());
        dep_info.push_str(&source_path.to_string_lossy());
        dep_info.push_str(":\n");
    }

    let dep_path = deps.join(dep_file_name);
    let dep_result = fs::write(&dep_path, dep_info);
    assert!(dep_result.is_ok());
    dep_path
}

fn assert_dependency_safety(sources: &[(&str, &str)], expected: Safety) {
    let deps = create_test_dir();
    let dep_path = write_dependency(&deps, "example-123.d", sources);

    let safety_result = dependency_safety(&[dep_path]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example".to_string(), expected)]));
    }

    let cleanup_result = fs::remove_dir_all(&deps);
    assert!(cleanup_result.is_ok());
}

#[test]
fn reports_forbids_unsafe_from_dependency_entry_point() {
    assert_dependency_safety(&[("lib.rs", "#![forbid(unsafe_code)]\n")], Safety::ForbidsUnsafe);
}

#[test]
fn reports_no_unsafe_from_dependency_entry_point_without_forbid_attribute() {
    assert_dependency_safety(&[("lib.rs", "pub fn value() -> usize { 0 }\n")], Safety::NoUnsafe);
}

#[test]
fn reports_unsafe_usage_from_dependency_entry_point() {
    assert_dependency_safety(&[("lib.rs", "pub unsafe fn value() {}\n")], Safety::UsesUnsafe(1));
}

#[test]
fn reports_no_unsafe_from_multiple_source_files() {
    assert_dependency_safety(
        &[
            ("lib_0.rs", "pub fn value() -> usize { 0 }\n"),
            ("lib_1.rs", "pub fn other_value() -> usize { 1 }\n"),
        ],
        Safety::NoUnsafe,
    );
}

#[test]
fn reports_unsafe_usage_from_multiple_source_files() {
    assert_dependency_safety(
        &[
            ("lib_0.rs", "pub fn value() -> usize { 0 }\n"),
            ("lib_1.rs", "pub unsafe fn value() {}\n"),
        ],
        Safety::UsesUnsafe(1),
    );
}

#[test]
fn skips_non_source_files_with_unsafe_content() {
    assert_dependency_safety(
        &[("example_0.rs", "pub fn value() {}\n"), ("README.md", "unsafe fn documented() {}\n")],
        Safety::NoUnsafe,
    );
}

#[test]
fn reports_io_error_for_non_existent_path() {
    let scan_err = dependency_safety(&[PathBuf::from("non-existent/example-123.d")]).unwrap_err();
    assert!(matches!(scan_err, ScanError::Io(_)));
    assert!(scan_err.to_string().starts_with("Failed to read source file"));
    assert!(scan_err.source().is_some());
}

#[test]
fn returns_no_dependencies_for_empty_input() {
    let safety_result = dependency_safety(&[]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert!(safety.is_empty());
    }
}

#[test]
fn reports_missing_entry_point_for_dependency_without_sources() {
    let deps = create_test_dir();
    let dep_path = write_dependency(&deps, "example-123.d", &[]);

    let scan_err = dependency_safety(&[dep_path]).unwrap_err();
    assert!(matches!(scan_err, ScanError::MissingEntryPoint));
    assert_eq!(scan_err.to_string(), "Entry point is not found");
    assert!(scan_err.source().is_none());

    let cleanup_result = fs::remove_dir_all(&deps);
    assert!(cleanup_result.is_ok());
}

#[test]
fn reports_forbids_unsafe_and_continues_to_next_dependency() {
    let deps = create_test_dir();
    let forbidden = write_dependency(
        &deps,
        "forbidden-123.d",
        &[
            ("forbidden_lib.rs", "#![forbid(unsafe_code)]\n"),
            ("forbidden_module.rs", "pub unsafe fn value() {}\n"),
        ],
    );
    let next = write_dependency(&deps, "next-123.d", &[("next_lib.rs", "pub fn value() {}\n")]);

    let safety_result = dependency_safety(&[forbidden, next]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety.len(), 2);
        assert_eq!(safety.get("forbidden"), Some(&Safety::ForbidsUnsafe));
        assert_eq!(safety.get("next"), Some(&Safety::NoUnsafe));
    }

    let cleanup_result = fs::remove_dir_all(&deps);
    assert!(cleanup_result.is_ok());
}

#[test]
fn combines_unsafe_usage_from_multiple_d_files_with_same_crate_name() {
    let deps = create_test_dir();
    let first = write_dependency(&deps, "example-123.d", &[("lib0.rs", "pub unsafe fn a() {}\n")]);
    let second = write_dependency(&deps, "example-456.d", &[("lib1.rs", "pub unsafe fn b() {}\n")]);

    let safety_result = dependency_safety(&[first, second]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example".to_string(), Safety::UsesUnsafe(2))]));
    }

    let cleanup_result = fs::remove_dir_all(&deps);
    assert!(cleanup_result.is_ok());
}

#[test]
fn reports_versioned_crate_name_from_registry_path() {
    let deps = create_test_dir();

    let registry_dir = deps.join(".cargo/registry/src/index.crates.io-123/example-0.1.0/src");
    let create_result = fs::create_dir_all(&registry_dir);
    assert!(create_result.is_ok());

    let source_path = registry_dir.join("lib.rs");
    let write_result = fs::write(&source_path, "pub fn value() -> usize { 0 }\n");
    assert!(write_result.is_ok());

    let dep_path = deps.join("example-123.d");
    let dep_result = fs::write(&dep_path, format!("{}:\n", source_path.to_string_lossy()));
    assert!(dep_result.is_ok());

    let safety_result = dependency_safety(&[dep_path]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example-0.1.0".to_string(), Safety::NoUnsafe)]));
    }

    let cleanup_result = fs::remove_dir_all(&deps);
    assert!(cleanup_result.is_ok());
}
