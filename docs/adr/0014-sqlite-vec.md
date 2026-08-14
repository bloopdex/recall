# ADR-0014 — Vector storage: sqlite-vec

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Phase 3 needs nearest-neighbor vector search over stored embeddings.
Options: sqlite-vec (vec0 virtual table), external vector stores (Qdrant
etc.), or brute-force cosine in Rust.

## Decision

- **sqlite-vec 0.1.9**, loaded via `sqlite3_auto_extension` **before**
  connection creation (rusqlite applies registered auto-extensions only to
  new connections — a registration-order bug that bit the first
  implementation is pinned by the tests).
- One vec0 table: `embeddings_vec(embedding float[384] distance_metric=cosine)`,
  rowid = memory_id. Canonical vectors live in the `embeddings` metadata
  table (BLOB); vec0 is a derived index synced by triggers — the same
  external-index pattern as FTS5 (ADR-0005).
- **Query shape:** the MATCH query must be the driving scan. Joining
  metadata inside the same statement lets SQLite reorder the plan at scale
  and emit NULL `distance` (regression-pinned by `tests/semantic_10k.rs`);
  model/version filtering therefore happens as a second lookup over the ≤k
  returned rowids.
- Degenerate vectors (NaN/Inf) are rejected at insert — they produce NULL
  distances that are hard to debug.

## Consequences

- Vector search stays inside the same SQLite file: one database, one
  backup, transactional with memory writes.
- Failure behavior: if the extension fails to load, `vec_enabled` is false
  and Recall degrades to keyword-only search with a warning — never a crash.

## Alternatives considered

- **External vector database** — rejected: a server contradicts
  local-first and adds operations burden.
- **Brute-force cosine in Rust** — rejected as the primary path: O(N)
  per query; vec0 scales to the 10k target trivially (measured ~9.8 ms
  at 10k vectors).
