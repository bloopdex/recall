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

# Search past solutions (SQLite FTS5 keyword search)
recall search "postgres connection pool"

# List recent memories
recall list --limit 20
```

## How it works

- **Capture first, enrich later.** Only the problem and solution are required. Project name, git branch/commit, changed files, and the working directory are captured automatically (best-effort — capture still works outside git).
- **Keyword search via SQLite FTS5.** Problem and error fields are weighted higher than background text; ties break by recency. Semantic/vector search arrives in Phase 3.
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
| 3 — Semantic Search | next | Local embeddings + sqlite-vec, re-ranking |
| 4 — Shell & Git Integration | planned | Capture from failed commands, git hooks |
| 5 — Projects & Lifecycle | planned | Project scoping, retention |
| 6 — Hardening | planned | <100ms @ 10k entries, redaction, encryption at rest |
| 7 — Ecosystem & Release | planned | DeployScore incident feed, CI failure capture |

## License

MIT
