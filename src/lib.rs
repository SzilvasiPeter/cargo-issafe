//! A library for detecting `unsafe` usage in dependency crates.
//!
//! [`scan_unsafe`] reads cargo's `.d` dependency files and reports, for each
//! crate, whether unsafe code is forbidden, absent, or present.

#![forbid(unsafe_code)]
use std::fs;
use std::path::{Path, PathBuf};

pub mod error;
pub use error::IsSafeError;

/// The `unsafe` status of a crate's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsafe {
    /// The crate has `#![forbid(unsafe_code)]`, so `unsafe` is blocked.
    Forbidden,
    /// The crate allows `unsafe`, but none is used.
    Absent,
    /// The crate uses `unsafe`.
    Present,
}

/// Scan dependency `.d` files under `deps_dir` for `unsafe` usage.
/// # Errors
/// Returns [`IsSafeError::Io`] if a dependency file or source file can't be read, or
/// [`IsSafeError::MissingEntryPoint`] if no `.rs` entry point is found.
pub fn scan_unsafe(deps_dir: impl AsRef<Path>) -> Result<Vec<(String, Unsafe)>, IsSafeError> {
    let mut results = Vec::new();
    for path in fs::read_dir(deps_dir.as_ref())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "d"))
    {
        // The crate name is the first dash-separated part of the file name, e.g. rustc_lexer-bfc1ea28fe193e21.d
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.split('-').next())
            .unwrap_or_default()
            .to_string();

        // Gather all source code files from the dependency (.d) file:
        // ```
        // /home/pszilvasi/ws/cargo-issafe/target/debug/deps/rustc_lexer-bfc1ea28fe193e21.d: <we ignore this>
        // /home/pszilvasi/ws/cargo-issafe/target/debug/deps/librustc_lexer-bfc1ea28fe193e21.rlib: <we ignore this>
        // /home/pszilvasi/ws/cargo-issafe/target/debug/deps/librustc_lexer-bfc1ea28fe193e21.rmeta: <we ignore this>
        //
        // /home/pszilvasi/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustc_lexer-0.1.0/src/lib.rs:
        // /home/pszilvasi/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustc_lexer-0.1.0/src/cursor.rs:
        // /home/pszilvasi/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustc_lexer-0.1.0/src/unescape.rs:
        // ```
        let dep_info = fs::read_to_string(&path)?;
        let sources = dep_info
            .lines()
            .map(str::trim)
            .filter_map(|line| line.split_once(':').map(|(registry_path, _)| registry_path.trim()))
            .map(PathBuf::from)
            // Crates sometimes include non source code (e.g. markdown, data) files, exclude them since they can't be tokenized
            .filter(|source| source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rs")))
            .collect::<Vec<_>>();

        // The source files list starts with the crate entry point, so the first .rs file is the root.
        let Some(entry_point) = sources.first() else {
            return Err(IsSafeError::MissingEntryPoint);
        };

        let entry = fs::read_to_string(entry_point)?;
        if entry.contains("#![forbid(unsafe_code)]") {
            results.push((name, Unsafe::Forbidden));
            continue;
        }

        // Read every source file and join their contents into a single string.
        let sources = sources.iter().map(fs::read_to_string).collect::<Result<Vec<_>, _>>()?;
        let joined = sources.join("\n");

        // Tokenize the joined source and count the number of `unsafe` keyword.
        let (count, _) = rustc_lexer::tokenize(&joined).fold((0, 0), |(count, offset), token| {
            let next_offset = offset + token.len;
            let is_unsafe = token.kind == rustc_lexer::TokenKind::Ident
                && joined.get(offset..next_offset) == Some("unsafe");
            (count + usize::from(is_unsafe), next_offset)
        });
        results.push((name, if count == 0 { Unsafe::Absent } else { Unsafe::Present }));
    }

    Ok(results)
}
