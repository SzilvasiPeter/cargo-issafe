lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

run:
    cargo run
