//! A library for detecting `unsafe` usage in dependency crates.
//!
//! It reads cargo's `.d` dependency files and reports if unsafe code is forbidden, absent, or present in the crate.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::IsSafeError;

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

/// Scan dependency `.d` files under `deps_dir` for `unsafe` usage.
/// # Errors
/// Returns [`IsSafeError::Io`] if a dependency file or source file can't be read, or
/// [`IsSafeError::MissingEntryPoint`] if no `.rs` entry point is found.
pub fn dependency_safety(deps_dir: impl AsRef<Path>) -> Result<Vec<(String, Safety)>, IsSafeError> {
    let mut results = Vec::new();
    for path in fs::read_dir(deps_dir.as_ref())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "d"))
    {
        // The crate name is the first dash-separated part of the file name, e.g. rustc_lexer-bfc1ea28fe193e21.d
        let crate_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.split('-').next())
            .unwrap_or_default()
            .to_string();

        // Gather all source code files from the dependency (.d) file:
        let dep_info = fs::read_to_string(&path)?;
        let sources: Vec<PathBuf> = dep_info
            .lines()
            .map(str::trim)
            .filter_map(|line| line.split_once(':').map(|(registry_path, _)| registry_path.trim()))
            .map(PathBuf::from)
            .filter(|source| source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rs")))
            .collect();

        let crate_safety = crate_safety_profile(&sources)?;
        results.push((crate_name, crate_safety));
    }

    Ok(results)
}

/// Scan the crate's source files for `unsafe` usage.
/// The first source file is the crate entry point and if it forbids unsafe code, then the crate is safe.
fn crate_safety_profile(sources: &[PathBuf]) -> Result<Safety, IsSafeError> {
    if sources.is_empty() {
        return Err(IsSafeError::MissingEntryPoint);
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

#[cfg(test)]
mod tests {
    use super::{count_unsafe, strip_comments};

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
    fn counts_only_unsafe_ident() {
        assert_eq!(count_unsafe("unsafe fn f() { unsafe {} }\n// unsafe\nunsafe_ident"), 2);
    }
}
