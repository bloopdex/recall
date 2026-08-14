# Recall — Development Guide

## Prerequisites

- Rust stable (MSVC toolchain on Windows)
- git (optional — needed only for git-context tests and capture metadata)

## Everyday commands

```powershell
cd codebase\recall

cargo fmt --all                        # format
cargo clippy --all-targets -- -D warnings   # lint (warnings denied in CI)
cargo test                             # full suite (unit + integration)
cargo build --release                  # release binary (target\release\recall.exe)
cargo run --release --example bench_search   # 10k-entry search baseline
```

## Test layers

| Layer | Where | What |
|---|---|---|
| Unit | `#[cfg(test)]` in `src/` | validation, query quoting, config, git detection |
| Database integration | `tests/migrations.rs` | migrations, persistence round-trip, FTS sync, ordering |
| CLI integration | `tests/capture_search.rs` | capture → search → list against the real binary |
| Git context | `tests/git_context.rs` | real temp repos: commit/branch, detached HEAD, empty repo, git missing |
| Failure | `tests/failure.rs` | corrupt DB, missing dirs, empty input |
| Security | `tests/security.rs` | banned-network-dependency guard, no env-var auto-collection |

All tests use temp directories/databases; none depend on personal repo state.

## Adding a migration

1. Add `sql/NNNN_name.sql` under `src/infrastructure/database/sql/`.
2. Register it in `migrations.rs` (append-only, sorted by version).
3. Add a test to `tests/migrations.rs` covering the upgrade path.

## Benchmarks

See [benchmarks.md](benchmarks.md). The target for Phase 6 is keyword search
across 10,000 entries in <100ms; Phase 1/2 establishes the baseline.

## Zero-network guarantee

No networking crates may enter the dependency tree
(`scripts/check_no_network.ps1` + `tests/security.rs` enforce this). The
binary's only external interaction is spawning `git` for metadata.
