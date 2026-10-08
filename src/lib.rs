//! A library for detecting `unsafe` usage in Rust crate dependencies.
//!
//! `cargo-issafe` parses cargo's `.d` dependency files to locate the source of each dependency.
//! It runs `cargo check --message-format=json` on the default target folder and scans
//! only the `.d` files listed as compiler artifacts, so stale files from removed
//! dependencies or changed features are ignored without losing the compile cache.
//! After, it classifies the safety policy as [`Safety::ForbidsUnsafe`], [`Safety::NoUnsafe`], or [`Safety::UsesUnsafe`] by counting occurrences of the `unsafe` keyword.
//!
//! The primary entry point is [`dependency_safety`], which scans the `.d` files
//! reported by `cargo check --message-format=json` and returns the safety
//! classification for each crate.
//!
//! [`Safety::ForbidsUnsafe`]: scan::Safety::ForbidsUnsafe
//! [`Safety::NoUnsafe`]: scan::Safety::NoUnsafe
//! [`Safety::UsesUnsafe`]: scan::Safety::UsesUnsafe
//! [`dependency_safety`]: scan::dependency_safety

#![forbid(unsafe_code)]
#![warn(clippy::print_stdout, clippy::print_stderr)]
pub mod error;
pub mod manifest;
pub mod scan;
pub mod tree;
