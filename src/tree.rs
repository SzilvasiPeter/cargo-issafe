//! Returns the dependency tree using the Cargo.lock file.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::BuildHasher;

use crate::error::TreeError;
use crate::scan::Safety;

const GREEN: &str = "\x1b[32m";
const BLUE: &str = "\x1b[34m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

type SafetyMap<S> = HashMap<String, Safety, S>;

/// A single package node in the dependency tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Package ID formatted as `<crate_name>-<version>` (the root crate drops the version info).
    pub id: String,
    /// Indices of this package's dependencies within [`DependencyTree::nodes`].
    pub dependencies: Vec<usize>,
}

/// A parsed Cargo.lock dependency tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyTree {
    /// All package nodes, indexed by position.
    pub nodes: Vec<Package>,
    /// Index of the root package (the crate itself) in `nodes`.
    pub root: usize,
}

/// Parses a Cargo.lock file's contents into a [`DependencyTree`].
///
/// # Arguments
/// * `root_crate` - The root crate name (in snake case) to start building the tree from.
/// * `lockfile` - The string content of the `Cargo.lock` file to parse.
///
/// # Errors
/// Returns a [`TreeError`] if the lockfile is invalid such as containing no packages,
/// a package block is missing its name or version, or a dependency does not match any package.
pub fn dependency_tree(root_crate: &str, lockfile: &str) -> Result<DependencyTree, TreeError> {
    let packages: Vec<&str> =
        lockfile.split("[[package]]").filter(|pkg| pkg.contains("name = ")).collect();
    if packages.is_empty() {
        return Err(TreeError::Empty);
    }

    let mut root = None;
    let nodes: Vec<Package> = packages
        .iter()
        .enumerate()
        .map(|(i, pkg)| {
            let name = get_value(pkg, "name = ").ok_or(TreeError::MissingField)?.replace('-', "_");
            let version = get_value(pkg, "version = ").ok_or(TreeError::MissingField)?;
            let id = if name == root_crate {
                root = Some(i);
                name
            } else {
                format!("{name}-{version}")
            };

            let dependencies = collect_deps(pkg)
                .into_iter()
                .map(|dep| resolve_dep(dep, &packages))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Package { id, dependencies })
        })
        .collect::<Result<_, _>>()?;

    let root = root.ok_or_else(|| TreeError::UnresolvedDependency(root_crate.to_string()))?;

    Ok(DependencyTree { nodes, root })
}

// Extract the value based on the key from a package block.
fn get_value<'a>(package: &'a str, key: &str) -> Option<&'a str> {
    package
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(key))
        .and_then(|line| line.split('"').nth(1))
        .filter(|value| !value.is_empty())
}

// Collect the dependencies from a package block.
fn collect_deps(package: &str) -> Vec<&str> {
    package
        .split_once("dependencies = [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(deps_str, _)| deps_str.lines().filter_map(|line| line.split('"').nth(1)).collect())
        .unwrap_or_default()
}

/// Resolves a single dependency entry (e.g. `"syn 2.0.119"`) to the index of the matching package.
///
/// Cargo.lock dependencies look like "name version source"; only the name and version is meaningful.
/// When the dependency has a version, the version must match.
/// If not, the return the first package with the matching name.
fn resolve_dep(dep: &str, packages: &[&str]) -> Result<usize, TreeError> {
    let mut parts = dep.split_whitespace();
    let name = parts.next().unwrap_or(dep);
    let version = parts.next();

    packages
        .iter()
        .position(|package| {
            get_value(package, "name = ") == Some(name)
                && version.is_none_or(|wanted| get_value(package, "version = ") == Some(wanted))
        })
        .ok_or_else(|| TreeError::UnresolvedDependency(dep.to_string()))
}

/// Formats the dependency tree as a string with safety labels.
///
/// Each line represents a package with its safety classification, indented by depth.
/// Already visited packages are not expanded again.
#[must_use]
pub fn format_tree<S: BuildHasher>(tree: &DependencyTree, safety_map: &SafetyMap<S>) -> String {
    let mut visited = HashSet::new();
    let mut output = String::new();
    let mut stack = vec![(tree.root, 0)];

    while let Some((idx, depth)) = stack.pop() {
        let node = &tree.nodes[idx];
        let id = &node.id;
        let Some(safety) = safety_map
            .get(id)
            // Safety map stores local crates without version while package ID keeps the version
            .or_else(|| safety_map.get(id.split_once('-').map_or(id.as_str(), |(name, _)| name)))
        else {
            continue;
        };

        let (label, color) = match safety {
            Safety::ForbidsUnsafe => ("safe".to_string(), GREEN),
            Safety::NoUnsafe => ("0 unsafe".to_string(), BLUE),
            Safety::UsesUnsafe(count) => (format!("{count} unsafe"), RED),
        };

        let indent = "  ".repeat(depth);
        writeln!(output, "{indent}- {id} {color}[{label}]{RESET}").ok();

        if !visited.insert(id.clone()) {
            continue;
        }

        for &dep_idx in node.dependencies.iter().rev() {
            stack.push((dep_idx, depth + 1));
        }
    }

    output
}
