//! Parses Cargo manifest files.

/// Returns the manifest paths of workspace members from `cargo metadata` output.
#[must_use]
pub fn manifest_paths(metadata: &str) -> Vec<&str> {
    metadata
        .split("\"manifest_path\"")
        .skip(1)
        .filter_map(|tail| {
            Some(tail.split_once(':')?.1.trim_start().strip_prefix('"')?.split_once('"')?.0)
        })
        .collect()
}

///
/// Returns the crate name from the `[package]` table section.
///
/// Returns [`None`] if the manifest has no `[package]` table, e.g.
/// if it is a virtual manifest, or the package missing the `name` field.
#[must_use]
pub fn package_name(manifest: &str) -> Option<&str> {
    manifest.split_once("[package]")?.1.lines().find_map(|line| {
        let (field, value) = line.split_once('=')?;
        (field.trim() == "name").then(|| value.trim().trim_matches(['"', '\'']))
    })
}

#[cfg(test)]
mod tests {
    use super::{manifest_paths, package_name};

    #[test]
    fn extract_package_name_with_whitespaces() {
        let cases = [
            "[package]\n\n name = \"value\" ",
            "[package]\n name = \"value\" ",
            "[package]\n name = \"value\"",
            "[package]\nname = \"value\"",
            "[package]\nname= \"value\"",
            "[package]\nname =\"value\"",
            "[package]\nname=\"value\"",
            "[package]\nname  =\"value\"",
            "[package]\nname  = \"value\"",
            "[package]\nname  =  \"value\"",
        ];

        for case in cases {
            assert_eq!(package_name(case), Some("value"));
        }
    }

    #[test]
    fn extract_package_name_with_single_quotes() {
        assert_eq!(package_name("[package]\nname = 'value'"), Some("value"));
        assert_eq!(package_name("[package]\nname='cargo-issafe'"), Some("cargo-issafe"));
    }

    #[test]
    fn package_name_returns_none() {
        let cases = ["name = \"value\"", "[package]\n"];

        for case in cases {
            assert_eq!(package_name(case), None);
        }
    }

    #[test]
    fn invalid_toml_syntax_still_extract_the_value() {
        let cases =
            ["[package]\nname = value", "[package]\nname= \"value", "[package]\nname = value\""];

        for case in cases {
            assert_eq!(package_name(case), Some("value"));
        }
    }

    #[test]
    fn extracts_manifest_paths_from_metadata() {
        let metadata = r#"{"packages":[{"name":"root","manifest_path":"/ws/Cargo.toml"},{"name":"member","manifest_path": "/ws/member/Cargo.toml"}]}"#;
        assert_eq!(manifest_paths(metadata), ["/ws/Cargo.toml", "/ws/member/Cargo.toml"]);

        let empty = [] as [&str; 0];
        assert_eq!(manifest_paths(r#"{"packages":[]}"#), empty);
    }
}
