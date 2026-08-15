# Recall — Architecture

## Overview

Recall is a single-process local CLI: a thin `main` binary over a library crate.

```
recall (bin) ──► cli (clap) ──► application (capture / search / list / shell / git_hooks)
                                   │                │
                                   ▼                ▼
                             domain::memory     infrastructure::database (SQLite + FTS5 + vec0)
                             domain::sanitize   infrastructure::git (metadata + hook lifecycle)
                                                infrastructure::shell (prompt-hook snippets)
```

Phase 4 adds two observation paths around the existing capture flow:
the shell prompt hook records failed commands into env vars
(`recall capture --from-shell`), and a post-commit git hook invokes
`recall capture --from-git` after successful commits (ADR-0017/0019).

## Boundaries and dependency direction

- `domain/` — the canonical entry model (`NewMemory`, `Memory`). Pure data +
  validation. Knows nothing about SQLite, git, or the CLI.
- `infrastructure/database/` — SQLite persistence, migrations, FTS5. The only
  module allowed to touch SQL.
- `infrastructure/git/` — best-effort git metadata via safe subprocess calls.
- `application/` — workflows composing domain + infrastructure (capture flow,
  search flow, list flow). No SQL, no clap here.
- `cli/` — clap definitions + dispatch. Thin: parses, resolves config, calls
  the application layer.

Dependency inversion is used only where it earns its keep: the application
layer takes `&Db` as a parameter, and tests inject temporary database paths
rather than mock interfaces.

## Deliberate non-features (Phase 1/2)

- **No async, no tokio.** A local CLI has nothing to wait on. If a future
  phase (e.g. the Phase 4 shell integration) genuinely needs concurrency,
  revisit then.
- **No ORM.** The schema is small and stable; rusqlite's typed prepared
  statements are the right level of abstraction (ADR-002).
- **No embedded migrations framework** (refinery etc.). Eleven embedded SQL
  files + a 30-line runner cover the need (ADR-006).

## Key flows

**Capture** — resolve problem/solution (flag > piped stdin > prompt), detect
git/project context best-effort, normalize (trim, empty→None), validate
(problem+solution required), insert in one transaction, log
`capture.success` with `captures_count = 1`.

**Search** — quote each query term as an FTS5 string literal (injection-proof,
ADR-005), MATCH against `memories_fts`, rank by weighted bm25, tie-break by
capture time, log `search.run` with `search_duration_ms`.

## Decisions

- ADR-0000 — local-first
- ADR-0001 — project structure
- ADR-0002 — SQLite library
- ADR-0003 — SQLite configuration
- ADR-0004 — schema design
- ADR-0005 — FTS5 design
- ADR-0006 — migration strategy
- ADR-0007 — CLI architecture
- ADR-0008 — git metadata strategy
- ADR-0009 — error model
- ADR-0010 — zero-network enforcement
- ADR-0011 — deduplication (deterministic skip, not merge)
- ADR-0012 — edit command (user fields only)
- ADR-0013 — embedding model (all-MiniLM-L6-v2 via fastembed, local files)
- ADR-0014 — vector storage (sqlite-vec vec0, derived index)
- ADR-0015 — embedding storage & versioning (enrichment layer, rebuild)
- ADR-0016 — hybrid ranking (reciprocal-rank fusion)
- ADR-0017 — shell integration (prompt-hook observation, never a proxy)
- ADR-0018 — shell output sanitization & privacy (redact → show → confirm)
- ADR-0019 — git integration (non-blocking post-commit hook, explicit gate)
- ADR-0020 — hook installation & preservation (marked blocks, never overwrite)
