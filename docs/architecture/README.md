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

Two observation paths wrap the capture flow: the shell prompt hook
records failed commands into env vars (`recall capture --from-shell`),
and a post-commit git hook invokes `recall capture --from-git` after
successful commits (ADR-0017/0019).

Lifecycle and portability sit on the same pipeline: a single
`SearchFilter` (project + archived status) flows through FTS, semantic,
and hybrid search (ADR-0022/0023); `domain::export` defines the
portable JSON format; `application::lifecycle` and
`application::transfer` expose archive/unarchive/delete and
export/import; the pre-migration backup lives in `Db::open`
(ADR-0023/0006).

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

## Deliberate non-features

- **No async, no tokio.** A local CLI has nothing to wait on. If a future
  feature genuinely needs concurrency, revisit then.
- **No ORM.** The schema is small and stable; rusqlite's typed prepared
  statements are the right level of abstraction (ADR-0002).
- **No embedded migrations framework** (refinery etc.). Embedded SQL
  files + a small runner cover the need (ADR-0006).

## Key flows

**Capture** — resolve problem/solution (flag > piped stdin > prompt), detect
git/project context best-effort, normalize (trim, empty→None), validate
(problem+solution required), insert in one transaction, log
`capture.success` with `captures_count = 1`.

**Search** — quote each query term as an FTS5 string literal (injection-proof,
ADR-0005), MATCH against `memories_fts`, rank by weighted bm25, tie-break by
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
- ADR-0021 — project identity (user-facing label; no projects table)
- ADR-0022 — project-aware search (global default, one filtered pipeline)
- ADR-0023 — lifecycle (archive/delete, no automatic retention, pre-migration backup)
- ADR-0024 — export/import (portable JSON, no ids, secrets redacted by default)
- ADR-0025 — performance strategy (measure at scale, optimize on evidence)
- ADR-0026 — encryption at rest (rejected, with revisit conditions)
- ADR-0027 — concurrency, crash safety, and the recovery model
- ADR-0028 — `recall check` (read-only diagnostics, no auto-repair)
- ADR-0029 — DeployScore integration (deferred, defined future boundary)
- ADR-0030 — CI failure capture (opt-in `recall capture --from-ci`)
- ADR-0031 — release & distribution strategy (versioned surfaces, local-first artifacts)
