# ADR-0021 — Project identity: the user-facing label, not a registry

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

Phase 5 turns the Phase 1 `project` text column into a deliberate search
and lifecycle concept. Two questions: what IS a project identity, and
does it need the normalized `projects` table that ADR-0004 deliberately
deferred?

## Decision

- **Identity = the user-facing label** captured by the Phase 2 detection
  rules (git top-level directory name, else the working directory name,
  else an explicit `--project`) — unchanged from Phases 1–4. Matching is
  case-insensitive at query time (`COLLATE NOCASE`).
- **No `projects` table.** The ADR-0004 deferral is resolved: every Phase
  5 capability derives from the text column — scoping is a WHERE clause
  on the existing index, `recall projects` is a GROUP BY, lifecycle
  operations are DELETEs/UPDATEs on `memories`. Normalization would add
  a migration, joins, and a registry to keep in sync for zero query
  benefit at personal scale (10k memories measured). Normalization for
  its own sake is rejected.
- **Rejected identity strategies:**
  - *Filesystem-path identity* — breaks when a repository is renamed,
    cloned, or moved; old memories silently detach from the project.
  - *Git remote URL identity* — privacy-sensitive (never collected, not
    even locally — documented in the privacy research), and absent from
    offline/local repositories.
  - *Generated identifiers* — would require a per-repository config file
    (Recall does not write into user repositories) or a registry (above).
- **Documented consequences of name-based identity:** renaming a
  repository directory starts a new label for future captures (old
  memories keep the old label — `recall projects` shows both; `--project`
  gives manual control). Worktrees share the top-level name → one
  identity (tested). Nested repositories use the innermost repo (git's
  own semantics, tested). Monorepos are one project per repository; finer
  granularity is manual via `--project`.
- **Privacy:** no new metadata is collected; remote URLs are explicitly
  never read (`git config --get remote.origin.url` is deliberately not
  used); the stored `repo_path`/`cwd` are the existing Phase 2 fields.

## Alternatives considered

See the rejected strategies above; a hybrid "remote URL + local path"
was also rejected — it fixes renames only partially while reintroducing
the privacy cost of remotes.

## Revisit conditions

If real usage shows cross-clone search ("memories from the same project
on another machine") becoming important, revisit with a user-managed
alias (`recall project rename`) before any automatic identity scheme.
