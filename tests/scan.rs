//! Integration tests for the scan module.

#![allow(clippy::unwrap_used)]
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use cargo_issafe::error::ScanError;
use cargo_issafe::scan::{CompiledUnit, Safety, compiled_units, dependency_safety};
use tempfile::TempDir;

/// Creates a `deps` directory inside a disposable temporary directory.
///
/// The directory must be named `deps` because that is the only place where
/// compiler artifacts are looked for.
fn create_deps_dir() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let deps = temp_dir.path().join("deps");
    let create_result = fs::create_dir(&deps);
    assert!(create_result.is_ok());

    (temp_dir, deps)
}

/// Writes the source files and the `.d` file describing a single compilation unit.
fn write_unit(deps: &Path, package_id: &str, dep: &str, sources: &[(&str, &str)]) -> CompiledUnit {
    let mut dep_info = String::new();
    for &(source_name, source) in sources {
        let source_path = deps.join(source_name);
        let write_result = fs::write(&source_path, source);
        assert!(write_result.is_ok());
        dep_info.push_str(&source_path.to_string_lossy());
        dep_info.push_str(":\n");
    }

    let dep_file = deps.join(dep);
    let dep_result = fs::write(&dep_file, dep_info);
    assert!(dep_result.is_ok());

    CompiledUnit { package_id: package_id.to_string(), dep_file }
}

/// Rebuilds the `cargo check --message-format=json` output that produced the given units.
fn check_output(units: &[CompiledUnit]) -> String {
    units
        .iter()
        .map(|unit| {
            let stem = unit.dep_file.file_stem().unwrap().to_string_lossy().into_owned();
            let artifact = unit.dep_file.with_file_name(format!("lib{stem}.rmeta"));
            format!(
                r#"{{"reason":"compiler-artifact","package_id":"{}","filenames":["{}"]}}"#,
                unit.package_id,
                artifact.to_string_lossy()
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Scans the units the way the CLI does, from the compiler output down to the safety map.
fn scan(units: &[CompiledUnit]) -> Result<HashMap<String, Safety>, ScanError> {
    dependency_safety(&compiled_units(&check_output(units)))
}

fn assert_dependency_safety(sources: &[(&str, &str)], expected: Safety) {
    let (_temp_dir, deps) = create_deps_dir();
    let unit = write_unit(&deps, "path+file:///ws#example@0.1.0", "example-123.d", sources);

    let safety_result = scan(&[unit]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example-0.1.0".to_string(), expected)]));
    }
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
            ("lib_1.rs", "pub unsafe fn value() -> usize { 0 }\n"),
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
    let (_temp_dir, deps) = create_deps_dir();
    let unit = CompiledUnit {
        package_id: "path+file:///ws#example@0.1.0".to_string(),
        dep_file: deps.join("example-123.d"),
    };

    let scan_err = scan(&[unit]).unwrap_err();
    assert!(matches!(scan_err, ScanError::Io(_)));
    assert!(scan_err.to_string().starts_with("Failed to read source file"));
    assert!(scan_err.source().is_some());
}

#[test]
fn returns_no_dependencies_for_empty_input() {
    let safety_result = scan(&[]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert!(safety.is_empty());
    }
}

#[test]
fn reports_missing_entry_point_for_dependency_without_sources() {
    let (_temp_dir, deps) = create_deps_dir();
    let unit = write_unit(&deps, "path+file:///ws#example@0.1.0", "example-123.d", &[]);

    let scan_err = scan(&[unit]).unwrap_err();
    assert!(matches!(scan_err, ScanError::MissingEntryPoint));
    assert_eq!(scan_err.to_string(), "Entry point is not found");
    assert!(scan_err.source().is_none());
}

#[test]
fn reports_forbids_unsafe_and_continues_to_next_dependency() {
    let (_temp_dir, deps) = create_deps_dir();
    let forbidden = write_unit(
        &deps,
        "path+file:///ws#forbidden@0.1.0",
        "forbidden-123.d",
        &[
            ("forbidden_lib.rs", "#![forbid(unsafe_code)]\n"),
            ("forbidden_module.rs", "pub unsafe fn value() {}\n"),
        ],
    );
    let next = write_unit(
        &deps,
        "path+file:///ws#next@0.1.0",
        "next-123.d",
        &[("next_lib.rs", "pub fn value() {}\n")],
    );

    let safety_result = scan(&[forbidden, next]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety.len(), 2);
        assert_eq!(safety.get("forbidden-0.1.0"), Some(&Safety::ForbidsUnsafe));
        assert_eq!(safety.get("next-0.1.0"), Some(&Safety::NoUnsafe));
    }
}

#[test]
fn combines_unsafe_usage_from_multiple_units_of_the_same_package() {
    let (_temp_dir, deps) = create_deps_dir();
    let package_id = "path+file:///ws#example@0.1.0";
    let first =
        write_unit(&deps, package_id, "example-123.d", &[("lib0.rs", "pub unsafe fn a() {}\n")]);
    let second =
        write_unit(&deps, package_id, "example-456.d", &[("lib1.rs", "pub unsafe fn b() {}\n")]);

    let safety_result = scan(&[first, second]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example-0.1.0".to_string(), Safety::UsesUnsafe(2))]));
    }
}

#[test]
fn keeps_packages_with_the_same_name_at_different_versions_apart() {
    let (_temp_dir, deps) = create_deps_dir();
    let older = write_unit(
        &deps,
        "path+file:///ws#example@0.1.0",
        "example-123.d",
        &[("lib0.rs", "pub unsafe fn a() {}\n")],
    );
    let newer = write_unit(
        &deps,
        "path+file:///ws#example@0.2.0",
        "example-456.d",
        &[("lib1.rs", "pub fn b() {}\n")],
    );

    let safety_result = scan(&[older, newer]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(
            safety,
            HashMap::from([
                ("example-0.1.0".to_string(), Safety::UsesUnsafe(1)),
                ("example-0.2.0".to_string(), Safety::NoUnsafe),
            ])
        );
    }
}

#[test]
fn reports_versioned_package_name_from_registry_dependency() {
    let (_temp_dir, deps) = create_deps_dir();
    let unit = write_unit(
        &deps,
        "registry+https://github.com/rust-lang/crates.io-index#example@0.1.0",
        "example-123.d",
        &[("lib.rs", "pub fn value() -> usize { 0 }\n")],
    );

    let safety_result = scan(&[unit]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example-0.1.0".to_string(), Safety::NoUnsafe)]));
    }
}

// Windows dep-info paths use drive-letter colons and backslashes (e.g. `D:\...\lib.rs:`).
// Only lines ending with `:` are sources, so the drive colon is never treated as a separator.
#[test]
fn reports_source_files_from_windows_dep_info_path() {
    let (_temp_dir, deps) = create_deps_dir();
    let unit = write_unit(
        &deps,
        "registry+https://github.com/rust-lang/crates.io-index#libc@0.2.177",
        "libc-123.d",
        &[(r"D:\cargo\registry\src\index\libc-0.2.177\src\lib.rs", "pub fn value() {}\n")],
    );

    let safety_result = scan(&[unit]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("libc-0.2.177".to_string(), Safety::NoUnsafe)]));
    }
}

// The `[lib]` and `[[bin]]` sections may rename a target, so the `.d` file stem holds the
// target name while `Cargo.lock` holds the package name. The `package_id` bridges the two.
#[test]
fn reports_package_name_for_renamed_lib_and_bin_targets() {
    let (_temp_dir, deps) = create_deps_dir();
    let package_id = "path+file:///ws#renamed-targets@0.1.0";
    let library =
        write_unit(&deps, package_id, "newlib-123.d", &[("lib.rs", "pub unsafe fn a() {}\n")]);
    let binary =
        write_unit(&deps, package_id, "newbin-456.d", &[("main.rs", "pub unsafe fn b() {}\n")]);

    let safety_result = scan(&[library, binary]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(
            safety,
            HashMap::from([("renamed-targets-0.1.0".to_string(), Safety::UsesUnsafe(2))])
        );
    }
}

#[test]
fn skips_units_without_a_resolvable_package_id() {
    let (_temp_dir, deps) = create_deps_dir();
    let resolvable = write_unit(
        &deps,
        "path+file:///ws#example@0.1.0",
        "example-123.d",
        &[("lib.rs", "pub fn value() {}\n")],
    );
    let broken = write_unit(&deps, "path+file:///ws#malformed", "malformed-456.d", &[]);

    let safety_result = scan(&[resolvable, broken]);
    assert!(safety_result.is_ok());
    if let Ok(safety) = safety_result {
        assert_eq!(safety, HashMap::from([("example-0.1.0".to_string(), Safety::NoUnsafe)]));
    }
}
