# Recall

**Personal engineering solution memory.** A local-first CLI that remembers how you solved engineering problems — and resurfaces the solution months later with one search.

```
$ recall search "postgres connection pool"

🔍 2 result(s) for "postgres connection pool"

1. PostgreSQL connection pool exhaustion on checkout-service
   📁 thorn-api  🕒 2026-08-14 15:30  id 42  commit a1b2c3d
   solution: Raised max_connections and enabled pgbouncer transaction pooling

2. ...
```

Everything stays on your machine: no network, no telemetry, no cloud.

### First run

The first command that creates your database greets you with a short
welcome: what Recall is, where your data lives, and the three commands
that matter. It prints once, only in a real terminal, and never blocks
or prompts — scripts and CI see nothing.

### Symbols

Interactive terminals get a small set of consistent icons (🧠 ✓ ✗ ⚠ →
🔒 🔍 💾 🌿 🖥 ⚙ 💡 📁 🕒). Piped output and scripts always get plain
ASCII instead — and `RECALL_PLAIN=1` forces plain output everywhere,
for terminals without unicode fonts.

## Quick start

```bash
cargo build --release

# Interactive capture (prompts for Problem and Solution)
recall capture

# Capture with flags
recall capture --problem "sqlite database is locked" --solution "set busy_timeout 5000" --error "database is locked (code 5)"

# Capture from a pipe
echo "TLS handshake timeout" | recall capture --solution "reused the connection pool"

# Near-identical re-capture is skipped deterministically (30-day window);
# override with --force
recall capture --problem "sqlite database is locked" --solution "..." --force

# Fix or clarify an existing memory (user fields only; FTS stays in sync)
recall edit 42 --solution "set busy_timeout 5000" --error ""   # empty clears

# Search past solutions (hybrid: FTS5 keyword + local semantic search)
recall search "postgres connection pool"
recall search --explain "postgres connection pool"   # ranking signals per result

# Semantic layer setup (one-time, the ONLY network step)
cargo install --path codebase/recall --features download   # build with download support
recall embeddings download                                  # fetch the local model
recall embeddings build                                     # backfill existing memories
recall embeddings status                                    # coverage report

# List recent memories
recall list --limit 20

# Shell integration (PowerShell / Bash): after a failed command, the prompt
# hook records command + exit code; capture with one prompt for the solution
recall shell install
some-command            # ...fails...
recall capture --from-shell        # problem pre-filled; secret patterns
                                   # are redacted + confirmed before saving
recall shell status                # installed / not installed
recall shell uninstall             # removes only recall's block

# Git integration: post-commit hook offers to capture each fix (explicit
# y/n, never automatic; non-interactive contexts skip instantly)
recall git install
git commit -m "fix: pool exhaustion"   # then confirm the capture prompt
recall git status / recall git uninstall
recall git install --append            # chain into an existing user hook

# Project-aware search (global by default; scope with --project)
recall search "connection pool"                # all projects
recall search --project thorn-api "connection pool"
recall projects                                # labels, counts, last activity
recall list --project thorn-api

# Lifecycle: archive = hide + keep (recoverable), delete = permanent
recall archive 42 / recall unarchive 42
recall search --include-archived "..."         # find what you archived
recall delete 42 --yes                         # terminal prompts instead
recall delete --project old-stack --yes        # bulk, confirmed

# Export/import: portable JSON, no internal ids, secrets redacted by default
recall export --path backup.json
recall export --include-secrets --path raw.json   # explicit opt-in
recall import backup.json                          # duplicates skipped

# Integrity + versioning
recall check        # read-only consistency checks (never repairs)
recall version      # app / schema / export-format / model versions
```

### CI failure capture (opt-in)

A GitHub Actions failure step pipes a bounded log tail into Recall and
names the remediation — see [docs/ci/github-actions.md](docs/ci/github-actions.md):

```yaml
- name: Capture failure in Recall
  if: failure()
  run: |
    tail -n 100 build.log | recall capture --from-ci \
      --step build --solution "re-run: this job flakes on cold caches"
```

## Installation

Release bundles contain the binary, checksums, and the install scripts
(see [docs/release/RELEASE-CHECKLIST.md](docs/release/RELEASE-CHECKLIST.md)):

```powershell
# Windows: from a release bundle directory (checksums verified)
powershell -File install.ps1 -From dist\recall-1.0.0-windows-x86_64
```

```bash
# Linux/macOS: from the per-OS build's release directory
sh install.sh /path/to/release-dir
```

Or build from source: `cargo install --path codebase/recall` (add
`--features download` if you want the model downloader).

The install script copies the binary into a user bin directory
(`~/.recall/bin` by default), verifies the SHA256 checksums when
present, then prints exactly what changed (one copied binary), how to
verify it (`recall version`), PATH guidance, and the next step. It
never touches PATH, shell profiles, integrations, your database, or the
model — those are separate, explicit steps (`recall shell install`,
`recall git install`, `recall embeddings download`).

### Uninstall

Four separate, explicit actions — an uninstall never deletes your
memories:

1. delete the installed binary (`~/.recall/bin/recall` or wherever it
   was installed);
2. `recall shell uninstall` — removes the prompt-hook block from your
   shell profile;
3. `recall git install`'s counterpart: `recall git uninstall` in each
   repository (user hook content is preserved);
4. only if you truly want to: delete the database file and the model
   directory yourself (`recall` never deletes them). Back up first:
   `recall export --path backup.json`.

## How it works

- **Capture first, enrich later.** Only the problem and solution are required. Project name, git branch/commit, changed files, and the working directory are captured automatically (best-effort — capture still works outside git).
- **Deduplication, deterministically.** A near-identical capture (same project + same normalized problem or error, within 30 days) is skipped with a clear message; `--force` overrides (ADR-0011).
- **Hybrid search: keyword + semantic.** FTS5 keyword search (weighted bm25) fuses with local semantic search (all-MiniLM-L6-v2 embeddings via fastembed, stored in sqlite-vec) using deterministic reciprocal-rank fusion — a paraphrase like "pool keeps running out of connections" finds "connections exhausted" memories even with zero keyword overlap (ADR-0013–0016). Fully offline: the model is a local file, fetched once by the opt-in `recall embeddings download`.
- **Shell integration observes; it never proxies.** The prompt hook records the last command + exit code after a failure (`RECALL_LAST_COMMAND` / `RECALL_LAST_EXIT_CODE` / `RECALL_LAST_CWD`); commands behave exactly as before. Output capture is explicit (pipe or `--error`), never retroactive (ADR-0017).
- **Git integration is structurally non-blocking.** A post-commit hook runs after the commit succeeds — a Recall failure can never abort a commit. Hooks install into the real hooks dir (worktree-aware), refuse to overwrite user hooks, and uninstall restores prior state (ADR-0019/0020).
- **Secrets are handled conservatively.** Auto-captured shell/git text is scanned for common secret shapes (password flags, tokens, `Bearer`/`Authorization` headers, AWS key ids, basic-auth URLs, PEM blocks); matches are redacted, shown, and require explicit confirmation before anything is stored (ADR-0018). Detection is not claimed to be perfect.
- **Local-first.** The database lives at `%LOCALAPPDATA%\recall\recall.db` (Windows) or `~/.local/share/recall/recall.db` (Linux/macOS). Override with `--db <path>` or `RECALL_DB_PATH`.

## Project structure

```
docs/                  architecture, database, design, development, ci, release, ADRs
codebase/recall/       the Rust crate (lib + bin)
  src/                 cli / application / domain / infrastructure (database, git, shell, ci)
  migrations (sql)     embedded SQLite migrations
  tests/               CLI, database, git, failure, security, concurrency, crash, upgrade tests
  examples/            bench_search / bench_projects / bench_scale (repeatable baselines)
scripts/               install scripts, release bundle script, benchmark + zero-network guard
fixtures/              example entries + upgrade fixtures (old-export compatibility)
.github/workflows/     CI (Windows + Linux: fmt, clippy, tests, download build, release build)
```

## Development

```bash
cd codebase/recall
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release --example bench_search
```

See [docs/development/README.md](docs/development/README.md) for the full workflow, [docs/development/DOGFOODING.md](docs/development/DOGFOODING.md) for the post-release dogfooding guide, and [docs/adr/](docs/adr/) for architecture decisions.

## Troubleshooting

- **"database file is corrupt or not a Recall database"** — Recall never
  modifies a damaged file. Diagnose with `recall check`; recover by
  restoring `<db>.pre-migration-backup` (taken before each schema
  upgrade) or re-importing `recall export` output, then
  `recall embeddings build`. Full recovery model: docs/database/README.md.
- **Semantic search missing / "embedding model not found"** —
  `recall embeddings status`, then `recall embeddings download` (the
  only network command) and `recall embeddings build`. Keyword search
  works regardless.
- **Capture says "Skipped: near-identical memory"** — deduplication
  working as designed; `--force` captures anyway.
- **Shell/git hook misbehaving** — `recall shell status` /
  `recall git status`; uninstall removes only Recall's marked blocks.
  A commit can never be blocked by Recall (the hook runs after, and
  skips without a terminal).
- **CI capture stored nothing and printed "Not saved"** — a
  secret-like pattern was detected in the piped log; CI fails closed
  by design. Sanitize the step's input (see docs/ci/github-actions.md).

## Security model

What is encrypted, what is redacted, what never leaves the machine
(ADR-0026/0018/0010/0027):

- **Encryption at rest: rejected, deliberately.** Recall keeps a
  plaintext SQLite database protected by OS account permissions.
  Researched (SQLCipher, SEE, OS keychains, passphrases, field-level
  encryption): every option either broke the zero-friction
  hook flows (passphrase prompts in non-interactive git hooks), gave
  protection no stronger than the OS account (key files next to the
  database), or forked the storage engine away from inspectable plain
  SQLite. Revisit conditions are recorded in ADR-0026.
- **Redaction is the primary defense.** Auto-captured context (failed
  commands, piped error output, commit subjects) passes through a
  conservative secret detector — flags (`--password=…`), key/value
  assignments (`DB_PASSWORD=…`), Bearer/Authorization headers, AWS key
  ids, basic-auth URLs, PEM blocks, JWTs, and GitHub/Slack/Stripe token
  shapes (ADR-0018). Detected secrets are shown redacted and require
  explicit confirmation before anything is stored. Exports redact by
  default (`--include-secrets` is opt-in). The guarantee is narrow and
  honest: common secret shapes never reach the database silently —
  not that arbitrary secrets are detectable.
- **Zero network, enforced in CI.** The default build has no network
  code path; the only network-capable code is the opt-in `recall
  embeddings download` feature. A dependency-tree scan runs inside
  `cargo test` on every platform (tests/security.rs), so CI fails if a
  network crate ever enters the tree.
- **Environment collection is whitelisted.** Recall reads exactly the
  three snapshot variables its own prompt hook writes
  (`RECALL_LAST_COMMAND`, `RECALL_LAST_EXIT_CODE`, `RECALL_LAST_CWD`)
  plus home-location variables; it never enumerates the environment
  (pinned by test).
- **Logs carry no content.** Tracing events record ids, counts, and
  metadata — never memory text or query terms (pinned by test).
- **Corruption fails loud.** A damaged database refuses to open with a
  message carrying the recovery model: `recall check` to diagnose,
  restore the `<db>.pre-migration-backup` snapshot, or re-import a
  `recall export`. Recall never auto-repairs your data.
- **Integrity is verifiable.** `recall check` runs SQLite's structural
  integrity check, the FTS5 integrity-check, and the engine-level
  invariants (no orphan embeddings, vec0 sync, valid lifecycle
  statuses) — read-only, scriptable exit code.

## Status

v1.0.0 is released and feature-complete: capture (interactive, flags,
stdin, shell, git, CI), hybrid keyword + semantic search, projects and
lifecycle management, portable export/import, integrity checks, and
hardening (concurrency, crash recovery, secret redaction, zero network).
The two deliberate deferrals — encryption at rest (ADR-0026) and the
DeployScore incident feed (ADR-0029) — are recorded with revisit
conditions.

## License

MIT
