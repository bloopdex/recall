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
