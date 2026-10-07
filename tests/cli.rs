//! Integration tests for CLI tool.

#![allow(clippy::unwrap_used)]
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use predicates::str::contains;

fn create_test_project(lib_rs: &str, cargo_toml: &str) -> PathBuf {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = env::temp_dir().join(format!("cargo-issafe-cli-test-{timestamp}"));
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(path.join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(path.join("src").join("lib.rs"), lib_rs).unwrap();
    path
}

#[test]
fn happy_path_project_with_no_dependencies() {
    let project_dir = create_test_project(
        "pub fn add(a: u32, b: u32) -> u32 { a + b }\n",
        "[package]\nname = \"my-package\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );

    Command::cargo_bin("cargo-issafe").unwrap().current_dir(&project_dir).assert().success();

    fs::remove_dir_all(&project_dir).ok();
}

#[test]
fn succeeds_when_target_cargo_issafe_already_exists() {
    let project_dir = create_test_project(
        "pub fn add(a: u32, b: u32) -> u32 { a + b }\n",
        "[package]\nname = \"target-exist\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );

    let stale_dir = project_dir.join("target/cargo-issafe");
    fs::create_dir_all(&stale_dir).unwrap();
    fs::write(stale_dir.join("stale_file"), "stale").unwrap();

    Command::cargo_bin("cargo-issafe").unwrap().current_dir(&project_dir).assert().success();

    assert!(!stale_dir.join("stale_file").exists());

    fs::remove_dir_all(&project_dir).ok();
}

#[test]
fn fails_with_fail_on_unsafe_when_dependency_uses_unsafe() {
    let project_dir = create_test_project(
        "pub fn add(a: u32, b: u32) -> u32 { a + b }\n",
        "[package]\nname = \"uses-unsafe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nlibc = \"0.2\"\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .arg("--fail-on-unsafe")
        .current_dir(&project_dir)
        .assert()
        .failure()
        .stderr(contains("the crate or its dependencies use unsafe code"));

    fs::remove_dir_all(&project_dir).ok();
}

#[test]
fn fails_with_cargo_check_failed_when_project_does_not_compile() {
    let project_dir = create_test_project(
        "pub fn add(a: u32, b: u32) -> u32 { a + }\n",
        "[package]\nname = \"broken-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(&project_dir)
        .assert()
        .failure()
        .stderr(contains("cargo check failed"));

    fs::remove_dir_all(&project_dir).ok();
}

// TODO: enable this test after workspace support is added
// #[test]
// fn supports_cargo_workspace() {
//     let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
//     let workspace_dir =
//         env::temp_dir().join(format!("cargo-issafe-cli-workspace-test-{timestamp}"));
//     fs::create_dir_all(workspace_dir.join("member/src")).unwrap();
//     fs::write(
//         workspace_dir.join("Cargo.toml"),
//         "[workspace]\nresolver = \"2\"\nmembers = [\"member\"]\n",
//     )
//     .unwrap();
//     fs::write(
//         workspace_dir.join("member/Cargo.toml"),
//         "[package]\nname = \"ws-member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
//     )
//     .unwrap();
//     fs::write(
//         workspace_dir.join("member/src/lib.rs"),
//         "pub fn add(a: u32, b: u32) -> u32 { a + b }\n",
//     )
//     .unwrap();
//
//     Command::cargo_bin("cargo-issafe")
//         .unwrap()
//         .current_dir(&workspace_dir)
//         .assert()
//         .success()
//         .stdout(contains("ws_member"));
//
//     fs::remove_dir_all(&workspace_dir).ok();
// }

#[test]
fn reports_safe_for_project_with_forbid_unsafe_and_serde_dependency() {
    let project_dir = create_test_project(
        "#![forbid(unsafe_code)]\npub fn value() -> u32 { 0 }\n",
        "[package]\nname = \"safe-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(&project_dir)
        .assert()
        .success()
        .stdout(contains("serde"));

    fs::remove_dir_all(&project_dir).ok();
}

#[test]
fn prints_path_dependency_under_root_crate() {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let project_dir = env::temp_dir().join(format!("cargo-issafe-cli-path-dep-test-{timestamp}"));
    fs::create_dir_all(project_dir.join("src")).unwrap();
    fs::create_dir_all(project_dir.join("helpers/src")).unwrap();
    fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"path-dep-root\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npath-helpers = { path = \"helpers\" }\n",
    )
    .unwrap();
    fs::write(project_dir.join("src/lib.rs"), "pub fn add() -> u32 { path_helpers::helper() }\n")
        .unwrap();
    fs::write(
        project_dir.join("helpers/Cargo.toml"),
        "[package]\nname = \"path-helpers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        project_dir.join("helpers/src/lib.rs"),
        "#![forbid(unsafe_code)]\npub fn helper() -> u32 { 1 }\n",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(&project_dir)
        .assert()
        .success()
        .stdout(contains("path_helpers-0.1.0"));

    fs::remove_dir_all(&project_dir).ok();
}
