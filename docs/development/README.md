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

See [benchmarks.md](benchmarks.md).

## Dogfooding

The post-release dogfooding guide — daily workflow, evidence rules,
bug-vs-behavior classification, and v1.1 criteria — lives in
[DOGFOODING.md](DOGFOODING.md).

- Search: `cargo run --release --example bench_search` (10k-entry FTS5,
  semantic, and hybrid baselines).
- Full-scale: `cargo run --release --example bench_scale -- [size]`
  (every operation at 10k/50k/100k with percentile distributions).
- Capture (in-process): `cargo run --release --example bench_capture`.
- Capture (end-to-end binary): `cargo test --release --test bench_capture -- --ignored --nocapture`.
- Embedding latency: `cargo run --release --example bench_embed`.
- Search quality: `cargo run --release --example eval_search`
  (Recall@5 / Precision@5 / MRR on the 24-memory corpus).

## Model-dependent tests

Real-model integration tests (`tests/embeddings_integration.rs` and
`tests/pooling_probe.rs`) skip gracefully when the model is not installed.
Install it once with: `cargo run --features download -- embeddings download`.

## Zero-network guarantee

No networking crates may enter the dependency tree
(`scripts/check_no_network.ps1` + `tests/security.rs` enforce this). The
binary's only external interaction is spawning `git` for metadata.

## Releasing

Versioning (ADR-0031): SemVer for the app (`recall version` shows the
four independent surfaces — app, database schema, export format,
embedding model). The reproducible release procedure is
[docs/release/RELEASE-CHECKLIST.md](../release/RELEASE-CHECKLIST.md);
the bundle script is `scripts/release.ps1 -Version <version>`. The
script generates the bundle into `dist/`, which is gitignored —
generated release artifacts are attached to the GitHub Release, never
committed to the repository. CI integration behavior is pinned by
`tests/ci_capture.rs` and documented in
[docs/ci/github-actions.md](../ci/github-actions.md); upgrade
compatibility is pinned by `tests/upgrade_paths.rs` against the
committed fixtures.
