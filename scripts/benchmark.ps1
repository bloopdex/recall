# Repeatable keyword-search baseline (10,000 entries).
# Run from the repository root.
Set-Location "$PSScriptRoot\..\codebase\recall"
cargo run --release --example bench_search
