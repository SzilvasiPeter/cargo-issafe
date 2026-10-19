//! Integration tests for CLI tool.

#![allow(clippy::unwrap_used)]
use std::fs;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt as _;
use predicates::str::contains;
use tempfile::TempDir;

fn create_test_project(lib_rs: &str, cargo_toml: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(dir.path().join("src").join("lib.rs"), lib_rs).unwrap();
    dir
}

#[test]
fn happy_path_project_with_no_dependencies() {
    let project_dir = create_test_project(
        "pub fn add(a: u32, b: u32) -> u32 { a + b }\n",
        "[package]\nname = \"my-package\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );

    Command::cargo_bin("cargo-issafe").unwrap().current_dir(project_dir.path()).assert().success();
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
        .current_dir(project_dir.path())
        .assert()
        .failure()
        .stderr(contains("the crate or its dependencies use unsafe code"));
}

#[test]
fn fails_with_cargo_check_failed_when_project_does_not_compile() {
    let project_dir = create_test_project(
        "pub fn add(a: u32, b: u32) -> u32 { a + }\n",
        "[package]\nname = \"broken-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .failure()
        .stderr(contains("the `cargo check` command failed"));
}

#[test]
fn single_member_workspace() {
    let workspace_dir = TempDir::new().unwrap();
    fs::create_dir_all(workspace_dir.path().join("member/src")).unwrap();
    fs::write(
        workspace_dir.path().join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"member\"]\n",
    )
    .unwrap();
    fs::write(
        workspace_dir.path().join("member/Cargo.toml"),
        "[package]\nname = \"ws-member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        workspace_dir.path().join("member/src/lib.rs"),
        "pub fn add(a: u32, b: u32) -> u32 { a + b }\n",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .arg("--workspace")
        .current_dir(workspace_dir.path())
        .assert()
        .success()
        .stdout(contains("ws-member"));
}

#[test]
fn multiple_members_workspace() {
    let workspace_dir = TempDir::new().unwrap();
    for member in ["first-member", "second-member"] {
        let member_dir = workspace_dir.path().join(member);
        fs::create_dir_all(member_dir.join("src")).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            format!("[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        fs::write(member_dir.join("src/lib.rs"), "pub fn value() -> u32 { 0 }\n").unwrap();
    }
    fs::write(
        workspace_dir.path().join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"first-member\", \"second-member\"]\n",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .arg("--workspace")
        .current_dir(workspace_dir.path())
        .assert()
        .success()
        .stdout(contains("first-member"))
        .stdout(contains("second-member"))
        .stdout(contains("---\n"));
}

#[test]
fn expands_the_dependencies_of_a_shared_crate_only_once() {
    let workspace_dir = TempDir::new().unwrap();
    for member in ["first-member", "second-member"] {
        let member_dir = workspace_dir.path().join(member);
        fs::create_dir_all(member_dir.join("src")).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ncommon = {{ path = \"../common\" }}\n"
            ),
        )
        .unwrap();
        fs::write(member_dir.join("src/lib.rs"), "pub fn value() -> u32 { 0 }\n").unwrap();
    }
    for (name, deps, source) in [
        ("common", "leaf = { path = \"../leaf\" }", "pub fn shared() -> u32 { 0 }\n"),
        ("leaf", "", "pub fn leaf() -> u32 { 0 }\n"),
    ] {
        let dir = workspace_dir.path().join(name);
        fs::create_dir_all(dir.join("src")).unwrap();
        let dependencies =
            if deps.is_empty() { String::new() } else { format!("\n[dependencies]\n{deps}\n") };
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{dependencies}"),
        )
        .unwrap();
        fs::write(dir.join("src/lib.rs"), source).unwrap();
    }
    fs::write(
        workspace_dir.path().join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"first-member\", \"second-member\"]\nexclude = [\"common\", \"leaf\"]\n",
    )
    .unwrap();

    let assert = Command::cargo_bin("cargo-issafe")
        .unwrap()
        .arg("--workspace")
        .current_dir(workspace_dir.path())
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert_eq!(output.matches("- common-0.1.0 ").count(), 2, "{output}");
    assert_eq!(output.matches("- leaf-0.1.0 ").count(), 1, "{output}");
}

#[test]
fn fails_without_workspace_flag_when_workspace_detected() {
    let workspace_dir = TempDir::new().unwrap();
    for member in ["first-member", "second-member"] {
        let member_dir = workspace_dir.path().join(member);
        fs::create_dir_all(member_dir.join("src")).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            format!("[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        fs::write(member_dir.join("src/lib.rs"), "pub fn value() -> u32 { 0 }\n").unwrap();
    }
    fs::create_dir_all(workspace_dir.path().join("src")).unwrap();
    fs::write(workspace_dir.path().join("src/lib.rs"), "pub fn value() -> u32 { 0 }\n").unwrap();
    fs::write(
        workspace_dir.path().join("Cargo.toml"),
        "[package]\nname = \"ws-root\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\nresolver = \"2\"\nmembers = [\"first-member\", \"second-member\"]\n",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(workspace_dir.path())
        .assert()
        .failure()
        .stderr(contains("cargo workspace is detected, use the `--workspace` argument"));
}

#[test]
fn prints_help_with_help_flag() {
    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage"))
        .stdout(contains("--fail-on-unsafe"))
        .stdout(contains("--workspace"));
}

#[test]
fn reports_safe_for_project_with_forbid_unsafe_and_serde_dependency() {
    let project_dir = create_test_project(
        "#![forbid(unsafe_code)]\npub fn value() -> u32 { 0 }\n",
        "[package]\nname = \"safe-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("serde"))
        .stdout(contains("serde_derive").not());
}

#[test]
fn prints_path_dependency_under_root_crate() {
    let project_dir = TempDir::new().unwrap();
    fs::create_dir_all(project_dir.path().join("src")).unwrap();
    fs::create_dir_all(project_dir.path().join("helpers/src")).unwrap();
    fs::write(
        project_dir.path().join("Cargo.toml"),
        "[package]\nname = \"path-dep-root\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npath-helpers = { path = \"helpers\" }\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("src/lib.rs"),
        "pub fn add() -> u32 { path_helpers::helper() }\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("helpers/Cargo.toml"),
        "[package]\nname = \"path-helpers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("helpers/src/lib.rs"),
        "#![forbid(unsafe_code)]\npub fn helper() -> u32 { 1 }\n",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("path-helpers-0.1.0"));
}

#[test]
fn succeeds_with_default_features() {
    let project_dir = create_test_project(
        "pub fn value() -> u32 {\n    tracing::info!(\"value\");\n    0\n}\n",
        "[package]\nname = \"default-features\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ntracing = \"0.1\"\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("tracing-attributes"));
}

#[test]
fn succeeds_with_default_features_disabled() {
    let project_dir = create_test_project(
        "#![forbid(unsafe_code)]\npub fn value() -> u32 { 0 }\n",
        "[package]\nname = \"no-default-features\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ntracing = { version = \"0.1\", default-features = false }\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("tracing-core"))
        .stdout(contains("tracing-attributes").not());
}

#[test]
fn succeeds_with_enabled_features() {
    let project_dir = create_test_project(
        "use serde::Serialize;\n#[derive(Serialize)]\npub struct Point {\n    x: u32,\n}\n",
        "[package]\nname = \"with-features\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
    );

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("serde_derive"));
}

#[test]
fn reports_root_crate_with_custom_binary_and_library_names() {
    let project_dir = TempDir::new().unwrap();
    fs::create_dir_all(project_dir.path().join("src")).unwrap();
    fs::write(
        project_dir.path().join("Cargo.toml"),
        "[package]\nname = \"renamed-targets\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[[bin]]\nname = \"newbin\"\npath = \"src/main.rs\"\n\n[lib]\nname = \"newlib\"\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    fs::write(project_dir.path().join("src/lib.rs"), "pub fn value() -> u32 { 0 }").unwrap();
    fs::write(
        project_dir.path().join("src/main.rs"),
        "fn main() {println!(\"{}\", newlib::value());}",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("renamed-targets-0.1.0").count(1))
        .stdout(contains("newlib").not())
        .stdout(contains("newbin").not());
}

#[test]
fn reports_root_crate_when_lib_and_bin_share_a_target_name() {
    let project_dir = TempDir::new().unwrap();
    fs::create_dir_all(project_dir.path().join("src")).unwrap();
    fs::write(
        project_dir.path().join("Cargo.toml"),
        "[package]\nname = \"renamed-targets\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"shared\"\npath = \"src/lib.rs\"\n\n[[bin]]\nname = \"shared\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("src/lib.rs"),
        "pub unsafe fn value() -> u32 { unsafe { 0 } }",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("src/main.rs"),
        "fn main() {println!(\"{}\", unsafe { shared::value() });}",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("renamed-targets-0.1.0").count(1))
        .stdout(contains("3 unsafe"));
}

#[test]
fn reports_path_dependency_with_a_renamed_library_name() {
    let project_dir = TempDir::new().unwrap();
    fs::create_dir_all(project_dir.path().join("src")).unwrap();
    fs::create_dir_all(project_dir.path().join("helpers/src")).unwrap();
    fs::write(
        project_dir.path().join("Cargo.toml"),
        "[package]\nname = \"path-dep-root\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npath-helpers = { path = \"helpers\" }\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("src/lib.rs"),
        "pub fn add() -> u32 { helpers_renamed::helper() }\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("helpers/Cargo.toml"),
        "[package]\nname = \"path-helpers\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"helpers_renamed\"\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    fs::write(
        project_dir.path().join("helpers/src/lib.rs"),
        "#![forbid(unsafe_code)]\npub fn helper() -> u32 { 1 }\n",
    )
    .unwrap();

    Command::cargo_bin("cargo-issafe")
        .unwrap()
        .current_dir(project_dir.path())
        .assert()
        .success()
        .stdout(contains("path-helpers-0.1.0").and(contains("safe")))
        .stdout(contains("helpers_renamed").not());
}
