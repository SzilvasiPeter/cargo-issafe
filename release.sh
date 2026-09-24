#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "Usage: $0 <major|minor|patch|X.Y.Z>"
    echo "  major  - bump major version (0.1.0 -> 1.0.0)"
    echo "  minor  - bump minor version (0.1.0 -> 0.2.0)"
    echo "  patch  - bump patch version (0.1.0 -> 0.1.1)"
    echo "  X.Y.Z  - set explicit version"
    exit 1
}

[[ $# -ne 1 ]] && usage

case "$1" in
    major|minor|patch) cargo set-version --bump "$1" ;;
    *) cargo set-version "$1" ;;
esac

# Refresh Cargo.lock with the new version and verify the build.
cargo check

version=$(cargo get package.version)
branch=$(git branch --show-current)

git add Cargo.toml Cargo.lock
git commit -m "release: $version"
git tag -a "v$version" -m "v$version"
git push origin "$branch" --tags

echo "Released v$version"
