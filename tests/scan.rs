//! Integration tests for the scan module.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use cargo_issafe::error::IsSafeError;
use cargo_issafe::scan::{Safety, dependency_safety};

fn create_test_dir() -> PathBuf {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = env::temp_dir().join(format!("cargo-issafe-tests-{}-{timestamp}", process::id()));
    let create_result = fs::create_dir(&path);
    assert!(create_result.is_ok());
    path
}

fn write_dependency(deps_dir: &Path, crate_name: &str, sources: &[(&str, &str)]) {
    let mut dep_info = String::new();
    for &(source_name, source) in sources {
        let source_path = deps_dir.join(source_name);
        let write_result = fs::write(&source_path, source);
        assert!(write_result.is_ok());
        dep_info.push_str(&source_path.to_string_lossy());
        dep_info.push_str(":\n");
    }

    let dep_path = deps_dir.join(format!("{crate_name}-123.d"));
    let dep_result = fs::write(dep_path, dep_info);
    assert!(dep_result.is_ok());
}

fn assert_dependency_safety(sources: &[(&str, &str)], expected: Safety) {
    let deps_dir = create_test_dir();
    write_dependency(&deps_dir, "example", sources);

    let safety_result = dependency_safety(&deps_dir);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, vec![("example".to_string(), expected)]);
    }

    let cleanup_result = fs::remove_dir_all(&deps_dir);
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
    let safety_result = dependency_safety("non-existent");
    assert!(matches!(safety_result, Err(IsSafeError::Io(_))));
}

#[test]
fn returns_no_dependencies_when_directory_has_no_d_files() {
    let deps_dir = create_test_dir();

    let safety_result = dependency_safety(&deps_dir);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert!(safety.is_empty());
    }

    let cleanup_result = fs::remove_dir_all(&deps_dir);
    assert!(cleanup_result.is_ok());
}

#[test]
fn reports_missing_entry_point_for_dependency_without_sources() {
    let deps_dir = create_test_dir();
    write_dependency(&deps_dir, "example", &[]);

    let safety_result = dependency_safety(&deps_dir);
    assert!(matches!(safety_result, Err(IsSafeError::MissingEntryPoint)));

    let cleanup_result = fs::remove_dir_all(&deps_dir);
    assert!(cleanup_result.is_ok());
}

#[test]
fn reports_forbids_unsafe_and_continues_to_next_dependency() {
    let deps_dir = create_test_dir();
    write_dependency(
        &deps_dir,
        "forbidden",
        &[
            ("forbidden_lib.rs", "#![forbid(unsafe_code)]\n"),
            ("forbidden_module.rs", "pub unsafe fn value() {}\n"),
        ],
    );
    write_dependency(&deps_dir, "next", &[("next_lib.rs", "pub fn value() {}\n")]);

    let safety_result = dependency_safety(&deps_dir);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety.len(), 2);
        assert!(safety.contains(&("forbidden".to_string(), Safety::ForbidsUnsafe)));
        assert!(safety.contains(&("next".to_string(), Safety::NoUnsafe)));
    }

    let cleanup_result = fs::remove_dir_all(&deps_dir);
    assert!(cleanup_result.is_ok());
}
