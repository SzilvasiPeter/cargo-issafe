//! Parses Cargo manifest files.

/// Returns the crate name from the `[package]` table section.
///
/// Returns [`None`] if the manifest has no `[package]` table, e.g.
/// if it is a virtual manifest, or the package missing the `name` field.
#[must_use]
pub fn package_name(manifest: &str) -> Option<&str> {
    manifest.split_once("[package]")?.1.lines().find_map(|line| {
        let (field, value) = line.split_once('=')?;
        (field.trim() == "name").then(|| value.trim().trim_matches('"'))
    })
}

#[cfg(test)]
mod tests {
    use super::package_name;

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
}
