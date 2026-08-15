# Contributing to Recall

Recall is a local-first personal engineering memory tool. Contributions
are welcome under the MIT license.

## Ground rules

- **Local-first is non-negotiable.** The default build must remain
  zero-network; the only sanctioned network path is the opt-in
  `recall embeddings download` feature. A full dependency-tree scan in
  the test suite enforces this — new network crates fail CI.
- **Tests are part of implementation.** New behavior ships with tests;
  security/privacy behavior (redaction, whitelists, fail-closed gates)
  is pinned by dedicated tests.
- **No silent privacy changes.** Redaction and confirmation flows follow
  ADR-0017/0018/0030; changes to what is captured or stored need an
  explicit, documented decision.
- **Migrations are append-only.** Never edit an applied migration; add
  a new version. Upgrades are covered by failure and backup-restore
  tests.
- **Architecture decisions are recorded.** Meaningful decisions get an
  ADR in `docs/adr/` (numbered, dated) — see ADR-0000 through ADR-0031
  for the established style.

## Development workflow

Prerequisites and everyday commands live in
[docs/development/README.md](docs/development/README.md). The short
version:

```bash
cd codebase/recall
cargo test --all-targets      # full suite (incl. zero-network scan)
cargo fmt --all -- --check    # formatting
cargo clippy --all-targets --all-features -- -D warnings
powershell -File ../scripts/check_no_network.ps1   # optional manual gate
```

Performance changes: run `cargo run --release --example bench_phase6`
before and after, and record the numbers in
`docs/development/benchmarks.md` (ADR-0025: measure → fix → measure).

## Commit conventions

Commit messages describe the change; every commit leaves the tree green
(tests, fmt, clippy). Release preparation follows
[docs/release/RELEASE-CHECKLIST.md](docs/release/RELEASE-CHECKLIST.md).
