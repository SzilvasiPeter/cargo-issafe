//! Returns the dependency tree using the Cargo.lock file.

use crate::error::TreeError;

/// A single package node in the dependency tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package<'a> {
    /// The package name.
    pub name: &'a str,
    /// The package version.
    pub version: &'a str,
    /// Indices of this package's dependencies within [`DependencyTree::nodes`].
    pub dependencies: Vec<usize>,
}

/// A parsed Cargo.lock dependency tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyTree<'a> {
    /// All package nodes, indexed by position.
    pub nodes: Vec<Package<'a>>,
    /// Index of the root package (the crate itself) in `nodes`.
    pub root: usize,
}

/// Parses a Cargo.lock file's contents into a [`DependencyTree`].
///
/// # Arguments
/// * `root` - The name of the package to start building the tree from.
/// * `lockfile` - The string content of the `Cargo.lock` file to parse.
///
/// # Errors
/// Returns a [`TreeError`] if the lockfile is invalid such as containing no packages,
/// a package block is missing its name or version, or a dependency does not match any package.
pub fn dependency_tree<'a>(root: &str, lockfile: &'a str) -> Result<DependencyTree<'a>, TreeError> {
    let packages: Vec<&str> =
        lockfile.split("[[package]]").filter(|pkg| pkg.contains("name = ")).collect();
    if packages.is_empty() {
        return Err(TreeError::Empty);
    }

    let nodes: Vec<Package> = packages
        .iter()
        .map(|package| {
            let name = extract_val(package, "name = ").ok_or(TreeError::MissingField)?;
            let version = extract_val(package, "version = ").ok_or(TreeError::MissingField)?;
            let dependencies = collect_deps(package)
                .into_iter()
                .map(|dep| resolve_dep(dep, &packages))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Package { name, version, dependencies })
        })
        .collect::<Result<_, _>>()?;

    let root = nodes
        .iter()
        .position(|node| node.name == root)
        .ok_or_else(|| TreeError::UnresolvedDependency(root.to_string()))?;

    Ok(DependencyTree { nodes, root })
}

/// Resolves a single dependency entry (e.g. `"syn 2.0.119"`) to the index of the matching package.
///
/// When the entry carries a version, the version must match as well.
/// If not, the first package with a matching name is used.
fn resolve_dep(dep: &str, packages: &[&str]) -> Result<usize, TreeError> {
    let (name, version) = parse_dep(dep);

    packages
        .iter()
        .position(|block| {
            extract_val(block, "name = ") == Some(name)
                && version.is_none_or(|wanted| extract_val(block, "version = ") == Some(wanted))
        })
        .ok_or_else(|| TreeError::UnresolvedDependency(dep.to_string()))
}

// TODO: we need to extract name and version together to get the unique identification, just cut the source part from it
/// Splits a dependency entry into its name and optional version.
///
/// Cargo.lock dependency entries look like "name", "name version", or "name version (source)";
/// only the name and version is meaningful.
fn parse_dep(dep: &str) -> (&str, Option<&str>) {
    let mut parts = dep.split_whitespace();
    let name = parts.next().unwrap_or(dep);
    (name, parts.next())
}

// Extract the value based on the key from a package block.
fn extract_val<'a>(package: &'a str, key: &str) -> Option<&'a str> {
    package
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(key))
        .and_then(|line| line.split('"').nth(1))
}

// Collect the dependencies from a package block.
fn collect_deps(package: &str) -> Vec<&str> {
    package
        .split_once("dependencies = [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(deps_str, _)| deps_str.lines().filter_map(|line| line.split('"').nth(1)).collect())
        .unwrap_or_default()
}

// TODO: create unit test for the private functions
