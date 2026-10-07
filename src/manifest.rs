//! Parses Cargo manifest files.

/// Returns the crate name from the `[package]` table of a manifest.
///
/// Returns [`None`] if the manifest has no `[package]` table, e.g. a virtual
/// workspace root, or if the table misses the `name` key.
#[must_use]
pub fn package_name(manifest: &str) -> Option<&str> {
    manifest.split("[package]").nth(1).and_then(|pkg| {
        pkg.lines()
            .map(str::trim)
            .find(|line| line.starts_with("name = "))
            .and_then(|line| line.split('"').nth(1))
    })
}
