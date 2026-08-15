# ADR-0003 — SQLite configuration

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

SQLite safety/durability settings must be chosen deliberately, not enabled
blindly.

## Decision

| Setting | Value | Rationale |
|---|---|---|
| foreign_keys | ON | correctness by default |
| journal_mode | WAL | readers never block the writer; crash-safe |
| synchronous | NORMAL | safe with WAL for single-process use; FULL adds fsync cost for nothing here |
| busy_timeout | 5000 ms | concurrent CLI invocations (two terminals) must not fail |

Applied on every open, before migrations. In-memory test databases skip WAL.

## Consequences

- WAL creates `-wal`/`-shm` sidecar files (gitignored).
- Durability trade-off: NORMAL can lose the last committed transaction on
  OS crash/power loss — acceptable for a personal memory tool; revisit if
  capture ever holds data that is not re-enterable.

## Alternatives considered

- **DELETE journal + FULL synchronous** — rejected: unnecessary fsync cost.
- **busy_timeout 0** — rejected: a second open would fail under contention.

## Amendment (Phase 6, 2026-08-15) — audit result: no changes

Phase 6 re-audited every pragma against concurrency and crash tests
before touching anything (the phase contract: do not change SQLite
settings without measuring or documenting the reason). Result: **the
configuration stands as decided.** WAL readers were proven to complete
behind a held write transaction; the 5 s busy timeout serialized 8
concurrent capture processes with zero lost writes; killed-mid-write
processes left the store healthy in every run. Two properties are now
documented as accepted limitations rather than bugs: `synchronous =
NORMAL` leaves a power-loss window of one transaction (fine for a
personal memory store), and SQLite's integrity_check detects structural
but not payload-content corruption (no page checksums — see ADR-0027).
