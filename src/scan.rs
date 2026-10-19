//! A library for detecting `unsafe` usage in dependency crates.
//!
//! It reads Cargo .d files to check if unsafe code is forbidden, absent, or present in a crate.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rustc_lexer::TokenKind;

use crate::error::ScanError;

/// The crate's `unsafe` code policy and usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    /// The crate has `#![forbid(unsafe_code)]`, so `unsafe` is blocked.
    ForbidsUnsafe,
    /// The crate allows `unsafe`, but none is used.
    NoUnsafe,
    /// The crate uses `unsafe` the given number of times.
    UsesUnsafe(usize),
}

/// A single compilation unit reported by `cargo check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledUnit {
    /// The package identity from the artifact's `package_id` field.
    pub package_id: String,
    /// The compiler's dependency (`.d`) file.
    pub dep_file: PathBuf,
}

/// Scan the given compilation units for `unsafe` usage.
///
/// Returns a map from package name with version to its safety classification.
/// Units of the same package share a `package_id` and merge into a single entry.
///
/// # Errors
/// Returns [`ScanError::Io`] if a file can't be read,
/// or [`ScanError::MissingEntryPoint`] if a `.d` file lists no `.rs` source.
pub fn dependency_safety(units: &[CompiledUnit]) -> Result<HashMap<String, Safety>, ScanError> {
    let mut results = HashMap::new();
    for unit in units {
        let Some(package_key) = lockfile_key(&unit.package_id) else {
            continue;
        };

        // Gather all source code files from the dependency (.d) file.
        // Only trailer lines (e.g. `src/lib.rs:`) hold source paths.
        let dep_info = fs::read_to_string(&unit.dep_file)?;
        let sources: Vec<PathBuf> = dep_info
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_suffix(':'))
            .map(PathBuf::from)
            .filter(|source| source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rs")))
            .collect();

        let crate_safety = crate_safety_profile(&sources)?;
        results
            .entry(package_key)
            .and_modify(|existing| *existing = merge_safety(*existing, crate_safety))
            .or_insert(crate_safety);
    }

    Ok(results)
}

/// Collect the compiled units from `cargo check --message-format=json` output.
#[must_use]
pub fn compiled_units(check_output: &str) -> Vec<CompiledUnit> {
    check_output
        .lines()
        .filter(|line| line.contains("\"reason\":\"compiler-artifact\""))
        .filter_map(artifact_unit)
        .collect()
}

/// Extract the `package_id` and the `.d` file from one `compiler-artifact` JSON line.
///
/// The fields are looked up independently, so their serialization order does not matter.
fn artifact_unit(line: &str) -> Option<CompiledUnit> {
    let package_id = artifact_package_id(line)?.to_string();
    let dep_file = artifact_dep_file(line)?;
    Some(CompiledUnit { package_id, dep_file })
}

/// Read the `package_id` field of a `compiler-artifact` JSON line.
fn artifact_package_id(line: &str) -> Option<&str> {
    let rest = line.split("\"package_id\"").nth(1)?;
    let value = rest.split_once(':')?.1.trim_start().strip_prefix('"')?;
    Some(value.split_once('"')?.0)
}

/// Extract the `.d` file from one `compiler-artifact` JSON line.
fn artifact_dep_file(line: &str) -> Option<PathBuf> {
    let rest = line.split("\"filenames\"").nth(1)?;
    let array = rest.split_once('[')?.1.split_once(']')?.0;
    array
        .split(',')
        .map(|entry| entry.trim().trim_matches('"'))
        .find_map(|entry| dep_file_from_artifact(Path::new(entry)))
}

/// Map a compiler artifact to its `.d` file.
///
/// E.g. `target/debug/deps/libserde-57b4d06e20d44d22.rmeta` maps to
/// `target/debug/deps/serde-57b4d06e20d44d22.d`.
#[must_use]
fn dep_file_from_artifact(artifact: &Path) -> Option<PathBuf> {
    // Loadable libraries: `.so` on Linux, `.dylib` on macOS, and `.dll` on Windows.
    if !matches!(artifact.extension()?.to_str()?, "rmeta" | "so" | "dylib" | "dll") {
        return None;
    }
    let parent = artifact.parent()?;
    if parent.file_name()?.to_str()? != "deps" {
        return None;
    }
    let stem = artifact.file_stem()?.to_str()?;
    let stripped = stem.strip_prefix("lib").unwrap_or(stem);
    Some(parent.join(format!("{stripped}.d")))
}

/// Convert a `package_id` into the `<package_name>-<version>` key used by the dependency tree.
///
/// The source origin (`registry+`, `path+`, `git+`, …) is irrelevant. Cargo omits the
/// `name@` part when it repeats the last path segment, which is common for workspace
/// members, so the name is then recovered from the path. Versions may contain dashes
/// and plus signs, so only the last `#` and the last `@` separate the fields.
///
/// Returns [`None`] if the identifier carries no name and no version.
#[must_use]
fn lockfile_key(package_id: &str) -> Option<String> {
    let (source, name_and_version) = package_id.rsplit_once('#')?;
    let (name, version) = match name_and_version.rsplit_once('@') {
        Some((name, version)) => (name, version),
        None => (source.rsplit(['/', '\\']).next()?, name_and_version),
    };

    if name.is_empty() || !version.starts_with(|char: char| char.is_ascii_digit()) {
        return None;
    }
    Some(format!("{name}-{version}"))
}

/// Scan the crate's source files for `unsafe` usage.
/// If the first file (entry point) forbids unsafe code, then the crate is safe.
fn crate_safety_profile(sources: &[PathBuf]) -> Result<Safety, ScanError> {
    if sources.is_empty() {
        return Err(ScanError::MissingEntryPoint);
    }

    let entry_content = fs::read_to_string(&sources[0])?;
    if forbids_unsafe(&entry_content) {
        return Ok(Safety::ForbidsUnsafe);
    }

    let mut count = count_unsafe(&entry_content);
    for source in &sources[1..] {
        count += count_unsafe(&fs::read_to_string(source)?);
    }

    Ok(if count == 0 { Safety::NoUnsafe } else { Safety::UsesUnsafe(count) })
}

/// Checks for the crate-level `#![forbid(unsafe_code)]` attribute.
///
/// Whitelist matcher: only the `#`, `!`, `[`, `forbid`, `(`, `unsafe_code`, `)`, `]`
/// token sequence counts. Whitespace and comments between tokens are ignored,
/// while strings, chars, and comments can never contribute a match.
#[must_use]
fn forbids_unsafe(source: &str) -> bool {
    let mut state: u8 = 0;
    let mut offset = 0;
    for token in rustc_lexer::tokenize(source) {
        let start = offset;
        offset += token.len;
        if matches!(
            token.kind,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        ) {
            continue;
        }
        let text = &source[start..offset];
        state = match (state, token.kind, text) {
            (0, TokenKind::Pound, _) => 1,
            (1, TokenKind::Not, _) => 2,
            (2, TokenKind::OpenBracket, _) => 3,
            (3, TokenKind::Ident, "forbid") => 4,
            (4, TokenKind::OpenParen, _) => 5,
            (5, TokenKind::Ident, "unsafe_code") => 6,
            (6, TokenKind::CloseParen, _) => 7,
            (7, TokenKind::CloseBracket, _) => return true,
            _ => u8::from(token.kind == TokenKind::Pound),
        };
    }
    false
}

/// Count the `unsafe` ident in the source code file's content.
fn count_unsafe(source: &str) -> usize {
    let mut count = 0;
    let mut offset = 0;
    for token in rustc_lexer::tokenize(source) {
        let start = offset;
        offset += token.len;
        if token.kind == TokenKind::Ident && &source[start..offset] == "unsafe" {
            count += 1;
        }
    }
    count
}

/// Merge two safety classifications of the same crate.
///
/// `UsesUnsafe` dominates everything (counts are added when both sides use `unsafe`),
/// `NoUnsafe` dominates `ForbidsUnsafe`, and equal variants merge into themselves.
const fn merge_safety(existing: Safety, recent: Safety) -> Safety {
    match (existing, recent) {
        (Safety::UsesUnsafe(old), Safety::UsesUnsafe(new)) => Safety::UsesUnsafe(old + new),
        (Safety::UsesUnsafe(_), _) => existing,
        (_, Safety::UsesUnsafe(_)) => recent,
        (Safety::NoUnsafe, _) | (_, Safety::NoUnsafe) => Safety::NoUnsafe,
        (Safety::ForbidsUnsafe, Safety::ForbidsUnsafe) => Safety::ForbidsUnsafe,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompiledUnit, Safety as SFT, artifact_dep_file, artifact_package_id, compiled_units,
        count_unsafe, dep_file_from_artifact, forbids_unsafe, lockfile_key, merge_safety,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn detects_forbid_unsafe_attribute() {
        assert!(forbids_unsafe("#![forbid(unsafe_code)]"));
        assert!(forbids_unsafe("#![/* comment */forbid(unsafe_code)]"));
        assert!(forbids_unsafe("# ! [ forbid ( unsafe_code ) ]"));
    }

    #[test]
    fn rejects_non_attribute_forbid() {
        for source in [
            "let s = \"#![forbid(unsafe_code)]\";",
            "// #![forbid(unsafe_code)]",
            "/* #![forbid(unsafe_code)] */",
            "#[forbid(unsafe_code)]",
        ] {
            assert!(!forbids_unsafe(source));
        }
    }

    #[test]
    fn counts_only_unsafe_keyword() {
        let unsafe_in_unsafe_fn = "unsafe fn f() { unsafe {} }\n// unsafe\nunsafe_ident";
        assert_eq!(count_unsafe(unsafe_in_unsafe_fn), 2);

        let unsafe_macro = "macro_rules! read_raw_ptr {
            ($ptr:expr) => {
                // unsafe { *$ptr } comment here
                unsafe { *$ptr }
            };
        }";
        assert_eq!(count_unsafe(unsafe_macro), 1);

        let no_mangle = "#[unsafe(no_mangle)]";
        assert_eq!(count_unsafe(no_mangle), 1);

        let raw_ident = "let r#unsafe = 10;";
        assert_eq!(count_unsafe(raw_ident), 0);
    }

    #[test]
    fn counts_pre_2024_ffi_without_unsafe_keyword() {
        for source in [
            "extern \"C\" { fn foo(); }",
            "#[no_mangle] pub extern \"C\" fn foo() {}",
            "#[export_name = \"foo\"] pub extern \"C\" fn bar() {}",
            "#[link_section = \".foo\"] static FOO: u32 = 1;",
        ] {
            assert_eq!(count_unsafe(source), 0,);
        }
    }

    #[test]
    fn counts_2024_ffi_with_unsafe_keyword() {
        for source in [
            "unsafe extern \"C\" { fn foo(); }",
            "#[unsafe(no_mangle)] pub fn foo() {}",
            "#[unsafe(export_name = \"foo\")] pub fn bar() {}",
            "#[unsafe(link_section = \".foo\")] static FOO: u32 = 1;",
        ] {
            assert_eq!(count_unsafe(source), 1);
        }
    }

    #[test]
    fn extracts_package_id_from_artifact() {
        let line = r#"{"package_id":"path+file:///ws#serde@1.0.229","filenames":["a.rmeta"]}"#;
        assert_eq!(artifact_package_id(line), Some("path+file:///ws#serde@1.0.229"));

        let missing = r#"{"filenames":["a.rmeta"]}"#;
        assert_eq!(artifact_package_id(missing), None);
    }

    #[test]
    fn builds_lockfile_key_from_package_id() {
        let cases = [
            ("registry+https://example.com#serde@1.0.229", "serde-1.0.229"),
            ("sparse+https://example.com#proc-macro2@1.0.95", "proc-macro2-1.0.95"),
            ("git+https://example.com#bar@0.1.0", "bar-0.1.0"),
            ("path+file:///ws#my-helper@0.1.0", "my-helper-0.1.0"),
            // The target is renamed, but the package name still comes from the lock file.
            ("path+file:///ws#renamed-targets@0.1.0", "renamed-targets-0.1.0"),
            // A directory that differs from the package name keeps the explicit `name@` part.
            ("path+file:///ws/crates/foo#totally-different@0.1.0", "totally-different-0.1.0"),
            // Workspace members repeat the package name as the last path segment, so Cargo
            // drops the `name@` part.
            ("path+file:///ws/first-member#0.1.0", "first-member-0.1.0"),
            // The version itself carries dashes and plus signs.
            ("registry+https://example.com#toml@1.1.8+spec-1.1.0", "toml-1.1.8+spec-1.1.0"),
        ];

        for (package_id, expected) in cases {
            assert_eq!(lockfile_key(package_id).as_deref(), Some(expected), "{package_id}");
        }
    }

    #[test]
    fn rejects_package_id_without_name_and_version() {
        let cases = [
            "registry+https://github.com/rust-lang/crates.io-index",
            "path+file:///home/user/ws",
            "path+file:///home/user/ws#my-helper",
            "path+file:///home/user/ws#my-helper@",
            "path+file:///home/user/ws#@0.1.0",
            "",
        ];

        for case in cases {
            assert_eq!(lockfile_key(case), None, "{case}");
        }
    }

    #[test]
    fn merge_safety_classification() {
        // Merging same safety classes
        assert_eq!(merge_safety(SFT::UsesUnsafe(1), SFT::UsesUnsafe(2)), SFT::UsesUnsafe(3));
        assert_eq!(merge_safety(SFT::NoUnsafe, SFT::NoUnsafe), SFT::NoUnsafe);
        assert_eq!(merge_safety(SFT::ForbidsUnsafe, SFT::ForbidsUnsafe), SFT::ForbidsUnsafe);

        // Unsafe usage dominates forbid or no unsafe usage.
        assert_eq!(merge_safety(SFT::UsesUnsafe(1), SFT::NoUnsafe), SFT::UsesUnsafe(1));
        assert_eq!(merge_safety(SFT::NoUnsafe, SFT::UsesUnsafe(1)), SFT::UsesUnsafe(1));
        assert_eq!(merge_safety(SFT::UsesUnsafe(1), SFT::ForbidsUnsafe), SFT::UsesUnsafe(1));
        assert_eq!(merge_safety(SFT::ForbidsUnsafe, SFT::UsesUnsafe(1)), SFT::UsesUnsafe(1));

        // No unsafe usage dominates forbid unsafe.
        assert_eq!(merge_safety(SFT::NoUnsafe, SFT::ForbidsUnsafe), SFT::NoUnsafe);
        assert_eq!(merge_safety(SFT::ForbidsUnsafe, SFT::NoUnsafe), SFT::NoUnsafe);
    }

    #[test]
    fn maps_proc_macro_artifacts_to_dep_files() {
        for artifact in [
            "target/debug/deps/libserde_derive-aaa.so",
            "target/debug/deps/libserde_derive-aaa.dylib",
            "target/debug/deps/serde_derive-aaa.dll",
        ] {
            assert_eq!(
                dep_file_from_artifact(Path::new(artifact)),
                Some(PathBuf::from("target/debug/deps/serde_derive-aaa.d")),
            );
        }
    }

    #[test]
    fn compiled_units_from_check_output() {
        let output = r#"{"reason":"compiler-artifact","package_id":"path+file:///ws#serde@1.0.229","filenames":["target/debug/deps/libserde-aaa.rmeta"]}
            {"reason":"compiler-artifact","package_id":"path+file:///ws#my-bin@0.1.0","filenames":["target/debug/deps/libmy_bin-bbb.rmeta"]}
            {"reason":"compiler-artifact","package_id":"path+file:///ws#spaced@0.1.0","filenames": ["target/debug/deps/libspaced-aaa.rmeta"]}
            {"reason":"compiler-artifact","package_id":"path+file:///ws#serde_derive@1.0.229","filenames":["target/debug/deps/libserde_derive-ccc.so"]}
            {"reason":"compiler-artifact","package_id":"path+file:///ws#syn@2.0.119","filenames":["target/debug/deps/libsyn-ddd.rlib","target/debug/deps/libsyn-ddd.rmeta"]}
            {"reason":"compiler-artifact","package_id":"path+file:///ws#serde@1.0.229","filenames":["target/debug/build/serde-eee/build-script-build"]}
            {"reason":"compiler-artifact","filenames":["target/debug/deps/liborphan-fff.rmeta"]}
            {"reason":"build-finished","success":true}
            not json"#;
        let unit = |package_id: &str, dep_file: &str| CompiledUnit {
            package_id: package_id.to_string(),
            dep_file: PathBuf::from(dep_file),
        };
        let ws = "path+file:///ws#";
        assert_eq!(
            compiled_units(output),
            vec![
                unit(&format!("{ws}serde@1.0.229"), "target/debug/deps/serde-aaa.d"),
                unit(&format!("{ws}my-bin@0.1.0"), "target/debug/deps/my_bin-bbb.d"),
                unit(&format!("{ws}spaced@0.1.0"), "target/debug/deps/spaced-aaa.d"),
                unit(&format!("{ws}serde_derive@1.0.229"), "target/debug/deps/serde_derive-ccc.d"),
                unit(&format!("{ws}syn@2.0.119"), "target/debug/deps/syn-ddd.d"),
            ]
        );
    }

    #[test]
    fn extracts_dep_file_from_artifact_line() {
        let line = r#"{"filenames":["target/debug/deps/libsyn-ddd.rlib","target/debug/deps/libsyn-ddd.rmeta"]}"#;
        assert_eq!(artifact_dep_file(line), Some(PathBuf::from("target/debug/deps/syn-ddd.d")));

        let no_artifact = r#"{"filenames":["target/debug/deps/libsyn-ddd.rlib"]}"#;
        assert_eq!(artifact_dep_file(no_artifact), None);
    }
}
