# ADR-0023 — Memory lifecycle: archive + delete, no automatic retention

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

The store must stay relevant as it grows — but Recall's core value is
long-term memory: an old fix is often the most valuable one. Lifecycle
needs a model that declutters without destroying history by default.

## Decision

- **Two lifecycle states** (migration 0003): `active` (searchable
  everywhere) and `archived` (kept, excluded from search by default,
  recoverable via `recall unarchive`).
  - Archive = hide, keep, recoverable. Delete = permanent.
  - Archived memories: excluded from FTS/semantic/hybrid search and from
    `recall list` by default; visible via `--include-archived` /
    `--archived`, marked as `archived` in results. Embeddings are kept
    (unarchive is instant, no rebuild). Deduplication still consults
    archived memories (archiving is deliberate; re-capturing must not
    recreate what was hidden).
- **Delete** (`recall delete <id>` / `recall delete --project <name>`):
  explicit, identifies what will be removed, confirmed at a TTY,
  requires `--yes` in non-interactive contexts (refusal otherwise —
  fail closed). Embeddings + both index entries are removed with the
  row via FK cascade and triggers — no orphaned vectors, tested.
- **No automatic retention.** Age is never a deletion reason; retention
  automation (scheduled archival) stays a Phase 6 concern per the
  original Phase 5 page. The lifecycle driver is explicit user action,
  assisted by `recall projects` (counts, last activity) and by archive
  as a reversible first step.
- **Archive was in scope here because the original Phase 5 page scoped
  it** ("archive (excluded from search) vs delete", "archived entries
  must not slow active search"). Archive-vs-delete research conclusion:
  delete alone would force a false choice between noise and history;
  the two-state model is the smallest design that offers both.
- **Pre-migration backup** (with this ADR, amending ADR-0006): before
  pending migrations are applied, `Db::open` snapshots the database to
  `<db>.pre-migration-backup` via SQLite's backup API (WAL-consistent,
  rolling, best-effort — a backup failure never blocks the open).
  Recovery: close Recall, restore the file. Tested with a seeded v2
  database.

## Alternatives considered

- **Archive-only, no delete** — rejected: users need a permanent option
  for genuinely obsolete entries (wrong-command noise, superseded
  stacks).
- **Delete-only, no archive** — rejected: hides-vs-destroys is a real
  distinction (see above).
- **Age-based automatic archival** — deferred to Phase 6; automatic
  deletion was never on the table.

## Consequences

A memory's default fate is permanence; every lifecycle change is
explicit and reversible (archive) or confirmed (delete). Nothing is
ever removed by Recall without the user deciding — in every
environment.
