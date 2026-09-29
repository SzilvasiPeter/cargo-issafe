lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo machete

lint-workflows:
    actionlint .github/workflows/*.yml
    shuck check --extend-select ALL --ignore S081 . # Ignore S081: file-header comments are noise inside `run:` blocks.
    zizmor .

coverage:
    cargo llvm-cov --all-features --fail-under-lines 80 --html --output-dir target/coverage
    cargo llvm-cov report --json --output-path target/coverage/coverage.json
    percent=$(jq -r '.data[0].totals.lines.percent' target/coverage/coverage.json) \
    && formatted=$(printf "%.2f%%" "$percent") \
    && jq -n --arg msg "$formatted" '{schemaVersion: 1, label: "coverage", message: $msg, color: "green"}' > target/coverage/badge.json

open:
    cargo llvm-cov --html --open

safety:
    cargo audit
    cargo issafe --fail-on-unsafe
    cargo deny check
