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
    // TODO: check the windows (.dll) and macos (dylib) in real build,
    // do i really need to add | "dylib" | "dll"?
    // https://rustc-dev-guide.rust-lang.org/backend/libs-and-metadata.html#rmeta
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
        let segment = line[..src_pos].rsplit('/').next()?;
        segment.match_indices('-').find_map(|(dash, _)| {
            let version = &segment[dash + 1..];
            version.as_bytes().first().is_some_and(u8::is_ascii_digit).then_some(version)
        })
    })
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
        Safety as SFT, compiled_dep_files, count_unsafe, extract_version, forbids_unsafe,
        merge_safety,
    };
    use std::path::PathBuf;

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
            assert!(!forbids_unsafe(source), "false positive: {source}");
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
    fn extracts_version_from_dep_info() {
        let dep_info = "
            home/ws/cargo-issafe/target/debug/deps/unicode_xid-39a96c518b5bd65a.d: home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs\n\n
            home/ws/cargo-issafe/target/debug/deps/libunicode_xid-39a96c518b5bd65a.rlib: home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs\n\n
            home/ws/cargo-issafe/target/debug/deps/libunicode_xid-39a96c518b5bd65a.rmeta: home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs\n\n
            home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/lib.rs:\n
            home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/unicode-xid-0.2.6/src/tables.rs:\n";
        assert_eq!(extract_version(dep_info), Some("0.2.6"));

        let double_hyphen = "/home/pszilvasi/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/toml-1.1.8+spec-1.1.0/src/lib.rs:";
        assert_eq!(extract_version(double_hyphen), Some("1.1.8+spec-1.1.0"));
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
    fn compiled_dep_files_from_check_output() {
        let output = r#"{"reason":"compiler-artifact","filenames":["target/debug/deps/libserde-aaa.rmeta"]}
            {"reason":"compiler-artifact","filenames":["target/debug/deps/libmy_bin-bbb.rmeta"]}
            {"reason":"compiler-artifact","filenames": ["target/debug/deps/libspaced-aaa.rmeta"]}
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
                PathBuf::from("target/debug/deps/spaced-aaa.d"),
                PathBuf::from("target/debug/deps/serde_derive-ccc.d"),
                PathBuf::from("target/debug/deps/syn-ddd.d"),
            ]
        );
    }
}
