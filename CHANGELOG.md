# Changelog

All notable changes to Recall are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project uses
[Semantic Versioning](https://semver.org/). The four versioned surfaces
are independent (ADR-0031): the application version here, the database
schema version (`recall version`), the export format version, and the
embedding model id/version.

## [1.0.0] — 2026-08-15

First release: capture, hybrid keyword + semantic search, shell/git/CI
integrations, projects and lifecycle management, portable export/import,
integrity checks, and hardening — shipped as a local release bundle with
checksum-verified install scripts.

### Core

- `recall capture` — interactive, flags, or piped stdin; automatic git
  context (branch, commit, changed files) and project detection;
  near-identical deduplication (30-day window, `--force` to override).
- `recall search` — hybrid search: FTS5 keyword + local semantic
  (all-MiniLM-L6-v2 via sqlite-vec), reciprocal-rank fusion, explainable
  per-engine signals; global by default, `--project` scoping,
  `--include-archived`.
- `recall list`, `recall edit`, `recall projects`.
- Lifecycle: `recall archive` / `unarchive` / `delete` (confirmed;
  `--yes` for non-interactive), embeddings cascade with the memory.
- Portable export/import: JSON envelope, no internal ids, secrets
  redacted by default (`--include-secrets` opt-in), duplicate detection
  with `--force`.
- `recall check` — read-only consistency diagnostics (structural
  integrity, FTS5 integrity-check, orphan detection, vec0 sync, status
  validity); non-zero exit when problems exist.
- `recall version` — the four versioned surfaces in one output.

### Integrations

- Shell integration (PowerShell/Bash): prompt-hook failure capture
  (`recall capture --from-shell`), explicit `recall shell install` /
  `uninstall` / `status`; never a command proxy; never breaks the shell.
- Git integration: non-blocking post-commit hook with an explicit
  per-commit gate (`recall git install [--append]` / `uninstall` /
  `status`); never overwrites user hooks; commits never depend on Recall.
- CI failure capture: opt-in GitHub Actions failure step
  (`recall capture --from-ci`), whitelisted `GITHUB_*` environment,
  fail-closed secret redaction, required `--solution`, deduplicates
  repeated failures.

### Privacy & security

- Zero network in the default build — enforced by a full dependency-tree
  scan in the test suite (runs in CI). The only network path is the
  explicit `recall embeddings download` (opt-in `download` feature).
- Conservative secret redaction for auto-captured context (flags,
  key/value assignments, Authorization headers, AWS key ids, basic-auth
  URLs, PEM blocks, JWTs, GitHub/Slack/Stripe tokens) with confirmation
  gates; exports redact by default.
- Environment collection is whitelisted and test-enforced; logs never
  carry memory content (test-enforced).
- Encryption at rest: researched and deliberately rejected — see
  ADR-0026 for the reasoning and revisit conditions.

### Reliability

- SQLite WAL + busy timeout + transactional writes; concurrency and
  crash/recovery suites (kill-mid-write, truncation, corruption,
  contention corners documented).
- Migrations: versioned, per-migration transactions, pre-migration
  backup (`<db>.pre-migration-backup`), failure atomicity, v1→v3 and
  v2→v3 upgrade tests, backup-restore tested end to end.
- Corruption fails loudly with the recovery model in the message;
  `recall check` verifies integrity.

### Performance (release, Windows 11, i5-14400F)

- Search <100 ms target verified at 10k entries: FTS 9.0 ms, semantic
  19.1 ms, hybrid 29.4 ms (median). Trends documented at 50k/100k in
  `docs/development/benchmarks.md` (ADR-0025).
- Process startup 12.0 ms median; capture ≈99 ms (git subprocess spawn
  cost on Windows, measured and documented).

## Known limitations

- SQLite pages carry no content checksums: structural corruption is
  detected, payload bit flips are not (ADR-0027).
- `synchronous = NORMAL`: a power loss in the instant of the last
  commit can lose that one transaction.
- Shell integration tested on PowerShell and Bash (generated for Zsh,
  not executed — ADR-0017).
- No distribution host configured yet: release artifacts are produced
  locally with checksums (never committed — see
  `docs/release/RELEASE-CHECKLIST.md`); publication awaits the hosting
  decision (ADR-0031).
