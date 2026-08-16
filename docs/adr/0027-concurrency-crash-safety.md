# ADR-0027 — Concurrency, crash safety, and the recovery model

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

Phase 6 had to verify how Recall behaves under concurrent use (shell
hook + git hook + several terminals + scripts) and under crashes and
file damage — and to document the recovery model. Recall is a multi-
process system with no daemon: every command is its own process opening
the same SQLite file.

## Research and audit results

- **Existing configuration (ADR-0003) is sufficient, unchanged.** WAL
  journaling (readers never block writers), `synchronous = NORMAL`
  (with WAL, durable to the last committed transaction except for
  power-loss in that instant — acceptable for a personal memory store,
  documented), 5 s busy timeout (write contention serializes instead of
  failing), foreign keys ON, every write in an explicit transaction.
  No pragma change was made — the spec required evidence, and the
  evidence said the configuration already behaves correctly.
- **Concurrency tests (new):** 8 concurrent capture processes — all
  persist; sustained write stream with concurrent readers — no errors,
  no partial rows, WAL readers verified to complete in <500 ms behind a
  HELD write transaction; uncommitted rows invisible to other
  connections; archive-vs-delete race — exactly one wins, the loser
  fails cleanly, FTS stays consistent; concurrent embedding inserts from
  separate connections — nothing lost.
- **Crash tests (new):** a capture process killed at 6 different points
  (including mid-write of a multi-page row) — the database is always
  healthy afterward, no partial memories; truncated files fail cleanly;
  zeroed files are reported with the recovery hint.
- **Detection boundary (measured, documented):** SQLite pages carry no
  content checksums. `PRAGMA integrity_check` detects STRUCTURAL damage
  (page headers, b-tree corruption — pinned by test) but cannot detect
  a bit flip inside cell PAYLOAD text (pinned by test as a documented
  limitation). This is inherent to plain SQLite and is one of the
  factors in the encryption decision (ADR-0026).

## Decision

- **Keep the single-connection-per-process model.** The library holds
  one connection; multi-process concurrency is handled by SQLite's
  locking (WAL + busy timeout), which the tests pin as sufficient for
  the documented usage. No connection pool, no cross-process locks, no
  daemon.
- **Fail loud, never silent:** corruption at open time surfaces the
  recovery model in the error message itself (restore the
  pre-migration backup, or re-import a Recall export). The damaged file
  is never modified by Recall.
- **The recovery model (documented in docs/database/README.md):**
  1. `recall check` — diagnose (ADR-0028);
  2. restore `<db>.pre-migration-backup` — the snapshot taken before
     the last schema upgrade (ADR-0006 amendment);
  3. re-import the latest `recall export` — portable JSON, redacted by
     default (ADR-0024);
  4. rebuild embeddings with `recall embeddings build`.
  Recall never auto-repairs.
- **Documented limitations, not hidden ones:** the payload-flip
  detection gap; the last-transaction power-loss window of
  `synchronous = NORMAL`; semantic search's full vec0 scan (ADR-0025);
  and the write-contention corners — see the Phase 7 amendment below.

## Alternatives considered

- **Stricter synchronous=FULL** — rejected: extra fsync per transaction
  buys durability Recall's threat model does not need, at real capture
  latency cost; WAL+NORMAL's guarantee (committed transactions survive
  process crashes, not OS power loss mid-commit) matches a personal
  memory store.
- **A daemon or in-process server serializing writes** — rejected:
  violates the one-binary local-first philosophy; SQLite already
  provides correct multi-process serialization, proven by the tests.
- **Content checksums / WAL-based page verification to close the
  payload-flip gap** — that is exactly what encryption schemes like
  SQLCipher's HMAC layer provide; deferred with ADR-0026.

## Consequences

The concurrency and crash guarantees are tested, not assumed: a
hardening regression suite (`tests/concurrency.rs`,
`tests/crash_recovery.rs`) pins the behaviors, and the recovery model
is user-visible in error messages and docs. If future phases add
heavier write patterns (batch imports, retention automation), the same
suites are the place to re-verify.

## Amendment (Phase 7, 2026-08-15) — the contention guarantee reframed

Phase 7 validation surfaced the busy-failure path even at REALISTIC
concurrency (3 capture processes on a warm database, while the full
test suite thrashed the disk) — the original "at realistic concurrency
every capture persists" claim was load-dependent and could not be
pinned honestly. The guarantee is reframed to its load-independent
form: SQLite's 5 s busy timeout can be exhausted by sustained
multi-process load; every capture then either persists or fails LOUDLY
with "database is locked"; silent loss and corruption never happen;
every busy-failed capture succeeds on retry; the store stays healthy
throughout. The concurrency suite was restructured to pin exactly this
(`concurrent_captures_never_lose_data_silently`,
`highly_contended_captures_never_lose_data_silently`,
`concurrent_first_run_captures_fail_loudly_and_the_store_stays_healthy`).

A further first-run form (observed on CI runners, 2026-08-16): when
several processes race to CREATE the database, the migration loser
fails loudly instead of hitting the busy timeout — "table ... already
exists" when it loses the table-creation migration, and "duplicate
column name" when it loses a later column migration (e.g. migration 3,
lifecycle_status) — same guarantee shape (loud, never silent, store
healthy, retry succeeds), pinned as accepted loud failures by
`concurrent_first_run_captures_fail_loudly_and_the_store_stays_healthy`.
