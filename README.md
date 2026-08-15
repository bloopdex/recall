# Recall

**Personal engineering solution memory.** A local-first CLI that remembers how you solved engineering problems — and resurfaces the solution months later with one search.

```
$ recall search "postgres connection pool"
#1  rank -6.42  captured 2026-08-14 15:30  id 42
    project: thorn-api
    commit:  a1b2c3d
    problem:  PostgreSQL connection pool exhaustion on checkout-service
    solution: Raised max_connections and enabled pgbouncer transaction pooling
```

Everything stays on your machine: no network, no telemetry, no cloud.

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
recall search --explain "postgres connection pool"   # show ranking signals

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
```

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
docs/                  architecture, database, design, development, ADRs
codebase/recall/       the Rust crate (lib + bin)
  src/                 cli / application / domain / infrastructure (database, git)
  migrations (sql)     embedded SQLite migrations
  tests/               CLI, database, git, failure, and security tests
  examples/            bench_search (repeatable 10k-entry baseline)
scripts/               benchmark + zero-network guard
fixtures/              example entries from the Phase 0 design
.github/workflows/     CI (Windows + Linux: fmt, clippy, tests, release build)
```

## Development

```bash
cd codebase/recall
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release --example bench_search
```

See [docs/development/README.md](docs/development/README.md) for the full workflow and [docs/adr/](docs/adr/) for architecture decisions.

## Roadmap

| Phase | Status | What |
|---|---|---|
| 0 — Data Model & UX Research | done | Entry schema, CLI surface, privacy design |
| 1 — Core Foundation | done | Rust project, SQLite + migrations, FTS5, error model, logging |
| 2 — Capture MVP | done | `recall capture` (interactive/flags/stdin), `recall search`, `recall list` |
| 3 — Semantic Search | done | Local embeddings (MiniLM via fastembed), sqlite-vec, hybrid RRF search, eval harness |
| 4 — Shell & Git Integration | done | Prompt-hook failure capture (PowerShell/Bash), post-commit git hook, secret redaction |
| 5 — Projects & Lifecycle | planned | Project scoping, retention |
| 6 — Hardening | planned | <100ms @ 10k entries, redaction, encryption at rest |
| 7 — Ecosystem & Release | planned | DeployScore incident feed, CI failure capture |

## License

MIT
