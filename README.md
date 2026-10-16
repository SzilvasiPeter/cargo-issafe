# cargo-issafe

[![crates.io](https://img.shields.io/crates/v/cargo-issafe)](https://crates.io/crates/cargo-issafe)
[![deps.rs](https://deps.rs/repo/github/SzilvasiPeter/cargo-issafe/status.svg)](https://deps.rs/repo/github/SzilvasiPeter/cargo-issafe)
[![coverage](https://img.shields.io/endpoint?url=https://szilvasipeter.github.io/cargo-issafe/badge.json)](https://szilvasipeter.github.io/cargo-issafe/html/index.html)
![forbids-unsafe](https://img.shields.io/badge/forbids-unsafe-blue)
![ci](https://github.com/SzilvasiPeter/cargo-issafe/actions/workflows/ci.yml/badge.svg)
![cd](https://github.com/SzilvasiPeter/cargo-issafe/actions/workflows/cd.yml/badge.svg)

A lightweight Rust CLI that detects unsafe keyword usage in crates and their dependencies.

## Why does this exist?

I care about writing safe and reliable code. This project was inspired by the [Cargo Geiger](https://github.com/geiger-rs/cargo-geiger) project, which scans for unsafe code. I have also realized that most crates (even the Rust standard library) have unsafe code or unsafe dependencies. Unsafe is like a sharp knife: if you can't use it, you will cut yourself, but it is an essential tool for survival. In some cases (e.g. FFI and hardware operations) it is unavoidable, because safe code builds on unsafe code, unsafe code bridges to C, and C talks to Linux. In other cases, unsafe usage is not justified and *may* cause undefined behaviors (UB) in the future. Since I am bad at handling sharp objects, I stripped down unsafe scanning into its minimal core so that the codebase and dependencies are safe:

- No unsafe code
- No unsafe dependencies [^1]
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

The output shows how many times the `unsafe` keyword appears in each dependency:

```
- cargo_issafe [safe]
  - rustc_lexer-0.1.0 [0 unsafe]
    - unicode_xid-0.2.6 [safe]
```

> [!NOTE]
> The "0 unsafe" means that the crate doesn't use unsafe code, but it does not forbid with the `#![forbid(unsafe_code)]` attribute.

Run with `--help` to see all arguments:

```
cargo-issafe - detect `unsafe` keyword usage in crates and dependencies

Usage: cargo issafe [--fail-on-unsafe] [--workspace]

Options:
    --fail-on-unsafe    fail when the crate or its dependencies use unsafe code
    --workspace         scan all workspace members
    --help              print this help message
```

If you found a lot of unsafe occurrences, please [don't panic](DONTPANIC.md).

## How does it work under the hood?

The tool runs `cargo check --message-format=json` and scans only the `.d` files listed as compiler artifacts, so removed dependencies and changed features leave no stale results. It then counts the `unsafe` keyword in each crate's sources and presents the findings.

## Limitations

- Conditional forbid via `cfg_attr`, e.g. `#![cfg_attr(not(feature = "allow-unsafe"), forbid(unsafe_code))]`, is not recognized: such crates report `0 unsafe` instead of `safe`.
- FFI related features such as `extern`, `no_mangle`, `export_name`, and `link_section` depend on the edition. Before edition 2024, they contain no `unsafe` keyword, hence counting zero unsafe usage, but since edition 2024 the same features require `unsafe` prefix.

## Contributing

Contributions are welcome. The goal is to keep the project small and simple, so when considering new features, please weigh the added functionality against increased maintainability burden and complexity.

## License

MIT

[^1]: The sole dependency is `rustc_lexer`, which is part of the Rust compiler, to tokenize unsafe keywords.
