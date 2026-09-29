# cargo-issafe

[![crates.io](https://img.shields.io/crates/v/cargo-issafe)](https://crates.io/crates/cargo-issafe)
[![coverage](https://img.shields.io/endpoint?url=https://szilvasipeter.github.io/cargo-issafe/badge.json)](https://szilvasipeter.github.io/cargo-issafe/html/index.html)
![forbids-unsafe](https://img.shields.io/badge/forbids-unsafe-blue)
![ci](https://github.com/SzilvasiPeter/cargo-issafe/actions/workflows/ci.yml/badge.svg)
![cd](https://github.com/SzilvasiPeter/cargo-issafe/actions/workflows/cd.yml/badge.svg)

A lightweight Rust CLI that detects unsafe keyword usage in crates and their dependencies.

## Why does this exist?

[Cargo Geiger](https://github.com/geiger-rs/cargo-geiger) gets the job done, but it carries a lot of bloat and waiting on maintainer reviews takes a while. It also surfaces far more detail than most people need, which increases its complexity.

cargo-issafe takes a different approach. It is based on a few principles:

- No unsafe code
- No external dependencies [^1]
- Small binary (under 500 KiB)
- Covered by thorough tests

## Usage

Install it with:

```sh
cargo install cargo-issafe
```

Run it from your crate:

```sh
cargo issafe
```

The output shows how many times the `unsafe` keyword appears in each dependency.

## How does it work under the hood?

The tool walks your dependency tree, counts unsafe ident, and presents the findings. There is no macro parsing or deep cargo integration involved.

Dependency resolution relies on `cargo check`, which is quicker than a full build. Once resolved, the relevant crate sources are pulled from the local cargo registry and scanned for unsafe usage.

## Contributing

Contributions are welcome.

## License

MIT

[^1]: The sole dependency is `rustc_lexer` which ships with the Rust standard library. While it does pull in unsafe dependencies (`memchr`, `unicode-ident`), neither cargo issafe nor cargo geiger can detect unsafe code within the Rust standard library itself.
