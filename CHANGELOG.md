# Changelog

All notable changes to Recall are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project uses
[Semantic Versioning](https://semver.org/). The four versioned surfaces
are independent (ADR-0031): the application version here, the database
schema version (`recall version`), the export format version, and the
embedding model id/version.

## [1.0.3] — 2026-08-16

First GitHub release: installation, first-run, and release-flow polish
(what was drafted as 1.0.1, plus the fixes that made the release
workflow pass on GitHub runners). No schema, ranking, privacy, or
contract changes.

### Added

- Tag-driven GitHub Releases: pushing a `vX.Y.Z` tag triggers
  `.github/workflows/release.yml`, which validates the tag against
  Cargo.toml, runs the full validation suite, builds the release
  bundle, verifies its checksums, and creates the GitHub Release with
  the bundle files attached (ADR-0031 Amendment 2).
- `scripts/uninstall.ps1` — removes the binary and the Recall user-PATH
  entry (every other entry preserved), never touches memories, the
  model, or the integrations.
- Friendly empty-store view for `recall list` on a brand-new database
  (interactive terminals only).

### Changed

- Windows installer now appends the bin directory to the USER PATH by
  default (`-SkipPath` opts out); entries are only ever appended and
  duplicates are never added; the installer reports exactly what
  changed and what did not (ADR-0031 Amendment).
- Default runs are quiet: the structured event log moved behind
  `--verbose`.

### Fixed

- Search results no longer print internal ranking scores by default;
  they are `--explain`-only.
- install.sh checksum verification now strips the leading `\` that GNU
  coreutils (MSYS2 / Git for Windows) print before the hash when the
  path contains backslashes — the installer no longer refuses a valid
  binary in that environment.
- Test-suite hardening for GitHub-hosted runners (test-only, no
  production behavior changed): the CI capture harness scrubs ambient
  `GITHUB_*` variables, PowerShell 5.1 test spawns run with a clean
  `PSModulePath`, the pooling probe skips when `LOCALAPPDATA` is
  absent, and the concurrency suite accepts the documented
  migration-race loud failure (ADR-0027).
- The shell-capture harness now scrubs the `RECALL_*` snapshot
  whitelist from spawned binaries: a sibling test mutating
  process-global env could leak a fake shell snapshot into the
  "no snapshot" case on parallel test threads (a runner-timing flake,
  test-only).

## [1.0.2] — 2026-08-16

Not released: the release workflow's test step hit a test-suite timing
flake (the shell-capture env leak above) on the runners. All of its
changes ship in 1.0.3.

## [1.0.1] — 2026-08-16

Not released: a release-candidate iteration whose tag never produced a
GitHub Release (the release workflow's test step failed on Windows
runners). All of its changes ship in 1.0.2.

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
- The repository is on GitHub (bloopdex/recall): pushing a `vX.Y.Z` tag
  triggers the release workflow (`.github/workflows/release.yml`),
  which validates, builds, and publishes the GitHub Release. Release
  artifacts are never committed (ADR-0031; procedure:
  `docs/release/RELEASE-CHECKLIST.md`).
