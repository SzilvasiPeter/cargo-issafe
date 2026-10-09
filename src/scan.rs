//! A library for detecting `unsafe` usage in dependency crates.
//!
//! It reads Cargo .d files to check if unsafe code is forbidden, absent, or present in a crate.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Scan the given dependency (`.d`) files for `unsafe` usage.
///
/// Returns a map from crate name with version to its safety classification.
///
/// # Errors
/// Returns [`ScanError::Io`] if a file can't be read,
/// or [`ScanError::MissingEntryPoint`] if a `.d` file lists no `.rs` source.
pub fn dependency_safety(paths: &[PathBuf]) -> Result<HashMap<String, Safety>, ScanError> {
    let mut results = HashMap::new();
    for path in paths {
        // Crate name is the part before the first dash, e.g. rustc_lexer-bfc1ea28fe193e21.d
        let crate_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.split('-').next())
            .unwrap_or_default()
            .to_string();

        // Gather all source code files from the dependency (.d) file.
        let dep_info = fs::read_to_string(path)?;
        let sources: Vec<PathBuf> = dep_info
            .lines()
            .map(str::trim)
            .filter_map(|line| line.split_once(':').map(|(registry_path, _)| registry_path.trim()))
            .map(PathBuf::from)
            .filter(|source| source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rs")))
            .collect();

        let version = extract_version(&dep_info);
        let crate_name = match version {
            Some(version) => format!("{crate_name}-{version}"),
            None => crate_name,
        };
        let crate_safety = crate_safety_profile(&sources)?;
        results
            .entry(crate_name)
            .and_modify(|existing| *existing = merge_safety(*existing, crate_safety))
            .or_insert(crate_safety);
    }

    Ok(results)
}

/// Collect the active `.d` files from `cargo check --message-format=json` output.
#[must_use]
pub fn compiled_dep_files(check_output: &str) -> Vec<PathBuf> {
    check_output
        .lines()
        .filter(|line| line.contains("\"reason\":\"compiler-artifact\""))
        .filter_map(artifact_dep_file)
        .collect()
}

/// Extract the `.d` file from one `compiler-artifact` JSON line.
fn artifact_dep_file(line: &str) -> Option<PathBuf> {
    let rest = line.split("\"filenames\":[").nth(1)?;
    let array = rest.split(']').next()?;
    array.split('"').find_map(|entry| dep_file_from_artifact(Path::new(entry)))
}

/// Map a compiler artifact to its `.d` file.
///
/// E.g. `target/debug/deps/libserde-57b4d06e20d44d22.rmeta` maps to
/// `target/debug/deps/serde-57b4d06e20d44d22.d`.
#[must_use]
fn dep_file_from_artifact(artifact: &Path) -> Option<PathBuf> {
    if !matches!(artifact.extension()?.to_str()?, "rmeta" | "so") {
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

/// Extract the crate version from a dependency (.d) file's content.
///
/// External registry dependencies always has version number, e.g.
/// the `0.2.6` version from `.cargo/registry/src/.../unicode-xid-0.2.6/src/lib.rs:` line.
/// On the other hand, local crates don't contain version information, e.g.
/// `src/main.rs:` or `my-helper/src/lib.rs:` lines.
fn extract_version(dep_info: &str) -> Option<&str> {
    dep_info.lines().find(|line| line.contains(".rs:")).and_then(|line| {
        let src_pos = line.rfind("/src")?;
        let dash_pos = line[..src_pos].rfind('-')?;
        let version = &line[dash_pos + 1..src_pos];
        version.as_bytes().first().is_some_and(u8::is_ascii_digit).then_some(version)
    })
}

/// Scan the crate's source files for `unsafe` usage.
/// If the first file (entry point) forbids unsafe code, then the crate is safe.
fn crate_safety_profile(sources: &[PathBuf]) -> Result<Safety, ScanError> {
    if sources.is_empty() {
        return Err(ScanError::MissingEntryPoint);
    }

    let entry_content = fs::read_to_string(&sources[0])?;
    if strip_comments(&entry_content).contains("#![forbid(unsafe_code)]") {
        return Ok(Safety::ForbidsUnsafe);
    }

    let mut count = count_unsafe(&entry_content);
    for source in &sources[1..] {
        count += count_unsafe(&fs::read_to_string(source)?);
    }

    Ok(if count == 0 { Safety::NoUnsafe } else { Safety::UsesUnsafe(count) })
}

/// Strip comments from the source code file's content.
fn strip_comments(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut offset = 0;
    for token in rustc_lexer::tokenize(source) {
        let start = offset;
        offset += token.len;
        if matches!(
            token.kind,
            rustc_lexer::TokenKind::LineComment | rustc_lexer::TokenKind::BlockComment { .. }
        ) {
            continue;
        }
        stripped.push_str(&source[start..offset]);
    }
    stripped
}

/// Count the `unsafe` ident in the source code file's content.
fn count_unsafe(source: &str) -> usize {
    let mut count = 0;
    let mut offset = 0;
    for token in rustc_lexer::tokenize(source) {
        let start = offset;
        offset += token.len;
        if token.kind == rustc_lexer::TokenKind::Ident && &source[start..offset] == "unsafe" {
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
        Safety as SFT, compiled_dep_files, count_unsafe, extract_version, merge_safety,
        strip_comments,
    };
    use std::path::PathBuf;

    #[test]
    fn removes_all_comment_styles() {
        assert_eq!(strip_comments("//! #![forbid(unsafe_code)]"), "");
        assert_eq!(strip_comments("/// #![forbid(unsafe_code)]"), "");
        assert_eq!(strip_comments("// #![forbid(unsafe_code)]"), "");
        assert_eq!(strip_comments("/* #![forbid(unsafe_code)] */"), "");
    }

    #[test]
    fn comment_inside_attribute_is_still_matched() {
        let source = "#![/* unsafe */forbid(unsafe_code)]";
        assert_eq!(strip_comments(source), "#![forbid(unsafe_code)]");
        assert_eq!(count_unsafe(source), 0);
    }

    #[test]
    fn ignores_comment_markers_in_strings_and_char_literals() {
        let source = "let url = \"http://example.com\"; let c = '/';";
        assert_eq!(strip_comments(source), source);
    }

    #[test]
    fn ignores_raw_identifier() {
        let raw_ident = "let r#unsafe = 10;";
        assert_eq!(count_unsafe(raw_ident), 0);
    }

    #[test]
    fn counts_only_unsafe_ident() {
        assert_eq!(count_unsafe("unsafe fn f() { unsafe {} }\n// unsafe\nunsafe_ident"), 2);
    }

    #[test]
    fn counts_unsafe_in_macro() {
        let macro_rule = "macro_rules! read_raw_ptr {
            ($ptr:expr) => {
                // The macro emits an unsafe block directly
                unsafe { *$ptr }
            };
        }";
        assert_eq!(count_unsafe(macro_rule), 1);
    }

    #[test]
    fn counts_unsafe_unsafe_nomangle() {
        let no_mangle = "#[unsafe(no_mangle)]";
        assert_eq!(count_unsafe(no_mangle), 1);
    }

    #[test]
    fn extracts_version_from_dep_info() {
        let dep_info = "
            home/ws/cargo-issafe/target/debug/deps/unicode_xid-39a96c518b5bd65a.d: home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs

            home/ws/cargo-issafe/target/debug/deps/libunicode_xid-39a96c518b5bd65a.rlib: home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs

            home/ws/cargo-issafe/target/debug/deps/libunicode_xid-39a96c518b5bd65a.rmeta: home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs

            home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs:
            home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs:
            ";

        assert_eq!(extract_version(dep_info), Some("0.2.6"));
    }

    #[test]
    fn ignores_directory_name_as_version() {
        let dep_info = "
            target/debug/deps/my_helpers-3b54bc15001c55ad.d: my-helpers/src/lib.rs

            my-helpers/src/lib.rs:
            ";

        assert_eq!(extract_version(dep_info), None);
    }

    #[test]
    fn merge_safety_adds_unsafe_counts() {
        assert_eq!(merge_safety(SFT::UsesUnsafe(1), SFT::UsesUnsafe(2)), SFT::UsesUnsafe(3));
    }

    #[test]
    fn merge_safety_unsafe_dominates() {
        assert_eq!(merge_safety(SFT::UsesUnsafe(1), SFT::NoUnsafe), SFT::UsesUnsafe(1));
        assert_eq!(merge_safety(SFT::NoUnsafe, SFT::UsesUnsafe(1)), SFT::UsesUnsafe(1));
        assert_eq!(merge_safety(SFT::UsesUnsafe(1), SFT::ForbidsUnsafe), SFT::UsesUnsafe(1));
        assert_eq!(merge_safety(SFT::ForbidsUnsafe, SFT::UsesUnsafe(1)), SFT::UsesUnsafe(1));
    }

    #[test]
    fn merge_safety_no_unsafe_dominates_forbids() {
        assert_eq!(merge_safety(SFT::NoUnsafe, SFT::ForbidsUnsafe), SFT::NoUnsafe);
        assert_eq!(merge_safety(SFT::ForbidsUnsafe, SFT::NoUnsafe), SFT::NoUnsafe);
    }

    #[test]
    fn merge_safety_equal_variants_merge_into_themselves() {
        assert_eq!(merge_safety(SFT::NoUnsafe, SFT::NoUnsafe), SFT::NoUnsafe);
        assert_eq!(merge_safety(SFT::ForbidsUnsafe, SFT::ForbidsUnsafe), SFT::ForbidsUnsafe);
    }

    #[test]
    fn compiled_dep_files_from_check_output() {
        let output = r#"{"reason":"compiler-artifact","filenames":["target/debug/deps/libserde-aaa.rmeta"]}
            {"reason":"compiler-artifact","filenames":["target/debug/deps/libmy_bin-bbb.rmeta"]}
            {"reason":"compiler-artifact","filenames":["target/debug/deps/libserde_derive-ccc.so"]}
            {"reason":"compiler-artifact","filenames":["target/debug/deps/libsyn-ddd.rlib","target/debug/deps/libsyn-ddd.rmeta"]}
            {"reason":"compiler-artifact","filenames":["target/debug/build/serde-eee/build-script-build"]}
            {"reason":"build-finished","success":true}
            not json"#;
        assert_eq!(
            compiled_dep_files(output),
            vec![
                PathBuf::from("target/debug/deps/serde-aaa.d"),
                PathBuf::from("target/debug/deps/my_bin-bbb.d"),
                PathBuf::from("target/debug/deps/serde_derive-ccc.d"),
                PathBuf::from("target/debug/deps/syn-ddd.d"),
            ]
        );
    }
}
