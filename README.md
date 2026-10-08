# cargo-issafe

[![crates.io](https://img.shields.io/crates/v/cargo-issafe)](https://crates.io/crates/cargo-issafe)
[![deps.rs](https://deps.rs/repo/github/SzilvasiPeter/cargo-issafe/status.svg)](https://deps.rs/repo/github/SzilvasiPeter/cargo-issafe)
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

If you want to fail when unsafe code is present, then use the `--fail-on-unsafe` argument.

The output shows how many times the `unsafe` keyword appears in each dependency:

```
- cargo_issafe [safe]
  - rustc_lexer-0.1.0 [0 unsafe]
    - unicode_xid-0.2.6 [safe]
```

> [!NOTE]
> The "0 unsafe" means that the crate doesn't use unsafe code, but it does not forbid it with the `#![forbid(unsafe_code)]` attribute.

## How does it work under the hood?

The tool runs `cargo check --message-format=json` and scans only the `.d` files listed as compiler artifacts, so removed dependencies and changed features leave no stale results. It then counts the `unsafe` keyword in each crate's sources and presents the findings.

## Contributing

Contributions are welcome.

## License

MIT

[^1]: The sole dependency is `rustc_lexer`, which is part of the Rust compiler, to tokenize unsafe keywords.
