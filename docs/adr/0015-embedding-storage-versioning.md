# ADR-0015 — Embedding storage & versioning

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Embedding models change. Stored vectors must carry enough metadata that a
model change can never silently mix incompatible vectors.

## Decision

- Enrichment layer table `embeddings` (migration 0002):
  `memory_id` (PK, FK CASCADE on memory delete), `model`, `model_version`,
  `dims`, `vector` (BLOB, little-endian f32), `created_at`.
- Current model: `MODEL_ID = "all-MiniLM-L6-v2"`, `MODEL_VERSION = "1"`
  — the version is a manual constant bumped when model files change.
- A memory without an embedding row is fully functional: keyword-searchable,
  editable, listable. Semantic indexing is enrichment (capture first,
  enrich later).
- **Backfill:** `recall embeddings build` embeds every memory whose
  embedding is missing or whose `model`/`model_version` differs from the
  current constants, in batches of 32, per-memory failures logged and
  retryable. `recall embeddings status` reports coverage.
- **Edit integration:** editing `problem`, `error`, or `context` (the
  embedded fields) regenerates the embedding immediately; if the model or
  store is unavailable, the old vector is **deleted** — a silently stale
  embedding must never survive. Editing other fields keeps the vector.
- **Capture integration:** new captures embed synchronously, best-effort;
  any failure degrades to "enrich later via build" and never blocks capture.
- Future model migration: bump `MODEL_VERSION` → old vectors count as
  stale → `embeddings build` rebuilds them without touching memories.

## Consequences

- Versioning is enforceable and testable (stale detection, rebuild,
  invalidation all covered by tests).
- The embeddings table is the source of truth; vec0 is always
  reconstructible from it.

## Alternatives considered

- **Store vectors without metadata** — rejected: model changes would
  silently mix incompatible vectors.
- **Async/background re-embedding** — rejected for Phase 3: a CLI has no
  background; synchronous best-effort is the simplest reliable option.
