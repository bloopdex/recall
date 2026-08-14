# ADR-0004 — Schema design

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Phase 0 froze a 9-field canonical entry model; the Phase 1/2 spec adds
optional fields (root cause, verification, environment, problem) and
auto-captured metadata (repo path, branch, changed files, cwd).

## Decision

One `memories` table with `problem` + `solution` NOT NULL and every other
column nullable. No separate `projects` table yet — `project` is a plain
text column (a real project entity arrives with Phase 5 scoping, as its
own migration). Timestamps are RFC3339 UTC strings with millisecond
precision (`captured_at`), plus DB-default `created_at`/`updated_at`.
Indexes on `project` and `captured_at`.

## Consequences

- Capture-first, enrich-later holds: any subset of optional fields can be
  missing.
- Adding Phase 3/5 tables is an ordinary migration, not a rewrite.
- No foreign keys needed in Phase 1/2 (the only table).

## Alternatives considered

- **projects + memories 1:N now** — rejected: Phase 5 decides project
  semantics; a text column migrates cleanly later.
- **JSON blob for optional fields** — rejected: unsearchable without
  effort; typed columns feed FTS5 directly.

## Field mapping (Phase 0 → schema)

Error→`error` · Context→`context` · Commands/Relevant files→
`investigation` · Git commit→`git_commit` · Solution→`solution` ·
Project→`project` · Timestamp→`captured_at` · Optional explanation→
`explanation`. New: `problem`, `root_cause`, `verification`,
`environment`, `repo_path`, `git_branch`, `git_changed_files`, `cwd`.
