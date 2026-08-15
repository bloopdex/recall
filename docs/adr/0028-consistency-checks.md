# ADR-0028 — `recall check`: read-only consistency diagnostics, no auto-repair

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

A memory store is only trustworthy if the user can verify it. Recall's
invariants (memory rows, FTS index, embedding metadata, vec0 vectors)
are maintained by SQLite mechanisms — FK cascade and triggers — but
nothing detects the state left behind by a failed trigger, a foreign
tool editing the file, or partial corruption. Phase 6 required
"consistency checks where useful" and allowed a diagnostic command only
if justified by research.

## Decision

- **`recall check` — one read-only command.** It runs:
  1. `PRAGMA integrity_check` (SQLite structural integrity);
  2. the FTS5 `integrity-check` command (index vs content table);
  3. embedding-orphan detection (embeddings rows whose memory row no
     longer exists);
  4. vec0 row-count agreement (trigger sync);
  5. lifecycle status validity (`active` / `archived` only).
  Note: no count-based FTS comparison — on an external-content FTS5
  table `count(*)` scans the CONTENT table, not the index, so counts
  cannot detect an index desync; the FTS5 integrity-check command can.
- **Report, never repair.** Problems are printed with names and counts,
  followed by the recovery model (ADR-0027). Exit code is non-zero when
  problems exist, so scripts can gate on `recall check`. Read-only is
  pinned by test (byte-identical database file before/after).
- **Detection boundary documented:** `integrity_check` verifies
  structure, not cell payload content — the same limitation pinned in
  ADR-0027's crash tests.
- A `repair` command was considered and rejected: auto-repair of a
  memory store is dangerous (repairing wrongly destroys history), and
  the recovery model (restore backup / re-import export) is already
  complete and tested.

## Alternatives considered

- **No command at all** — rejected: the phase spec explicitly asks for
  verifiable invariants; without a check command the concurrency and
  crash suites have no user-facing way to answer "is my store healthy?"
- **`recall repair` (auto-rebuild FTS/vec0 from the canonical table)** —
  rejected for now: rebuild paths exist (`recall embeddings build`
  covers vectors; a future FTS rebuild could be added), but
  automatically rewriting user data needs its own confirmation and
  backup discipline — revisit only if real-world corruption reports
  justify it.
- **Continuous background checking** — rejected: Recall is a one-shot
  CLI; checks run on demand.

## Consequences

One command verifies the whole store end to end; the hardening suites
use it to assert health after tampering, crashes, and races. Future
invariants (new tables, new statuses) get new checks in the same
place — and any check failure message must keep pointing at the
documented recovery path.
