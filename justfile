lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

open:
    cargo llvm-cov --html --open
