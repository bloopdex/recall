# ADR-0006 — Migration strategy

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

The schema must evolve (Phase 3 sqlite-vec, Phase 5 project entities)
without data loss, with zero runtime schema creation.

## Decision

Embedded SQL files (`sql/NNNN_name.sql`) compiled into the binary, applied
in version order at database open, one transaction per migration, recorded
in `schema_migrations(version, name, applied_at)`. Append-only list: an
applied migration is never edited; new schema = new version.

## Consequences

- A fresh install and an old install converge on the same schema
  automatically.
- A failed migration rolls back and leaves the DB at the previous version
  with a clear error.

## Alternatives considered

- **refinery / sqlx migrate** — rejected: extra dependencies for a
  30-line runner we already understand.
- **Schema-in-code (CREATE IF NOT EXISTS)** — rejected: no upgrade path
  for existing databases.

## Amendment (2026-08-15, ADR-0023)

Phase 5 adds a **pre-migration backup**: when `Db::open` finds pending
migrations on a file-backed database, it first snapshots the file to
`<db>.pre-migration-backup` using SQLite's backup API (WAL-consistent,
rolling — each upgrade replaces the previous backup, best-effort: a
backup failure logs and never blocks the open). Recovery path: close
Recall, restore the backup file over the database, reopen. The upgrade
of a seeded v2 database is covered by a migration test that verifies
both the data preservation and the backup's pre-upgrade schema.

## Amendment 2 (Phase 6, 2026-08-15) — migration hardening

Phase 6 extended the migration guarantee suite and documented the
recovery model (ADR-0027):

- **v1 → v3** upgrade covered (both pending migrations in one open; old
  data survives; embeddings + lifecycle work on the migrated rows).
- **Failure is atomic and retryable** — a migration that fails (pinned
  by a pre-existing conflicting column, the realistic case) rolls back
  its transaction, is not recorded in `schema_migrations`, leaves the
  file and data untouched, and succeeds after the conflict is resolved.
- **Recovery from backup is tested end to end** — destroy the database,
  restore `<db>.pre-migration-backup`, reopen: data back, upgrade
  re-applies cleanly.
- Migrations never silently destroy memories: the same guarantee as
  before, now with failure-path tests on both upgrade routes.
