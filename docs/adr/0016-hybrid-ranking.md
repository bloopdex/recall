# ADR-0016 — Hybrid ranking: reciprocal-rank fusion

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Hybrid search must combine bm25 (unbounded, lower-is-better) with cosine
similarity ([0,1]) without an opaque ML model, and every score must be
explainable.

## Decision

- **Reciprocal-rank fusion (RRF):** each engine returns up to 50 ranked
  candidates; the fused score of a memory is
  `fts_w/(60 + fts_position) + sem_w/(60 + sem_position)` with
  `fts_w = 1.0`, `sem_w = 0.9`, only present sides contributing.
  Final order: fused score desc, then captured_at desc, then id desc
  (deterministic tie-breaks).
- Semantic similarity (for display/`--explain`) is `1 - cosine_distance`
  clamped to [0,1]; the FTS contribution is the RRF of the bm25 position.
- RRF is chosen because bm25 and cosine scales are incommensurable and
  RRF is scale-free, deterministic, and explainable. Keyword weight is
  slightly ahead so exact matches stay on top when both engines agree;
  semantic-only candidates still surface (they are the point of Phase 3).
- **No confidence percentage.** A "97% confident" label would be
  uninterpretable; instead every result carries its measurable per-engine
  signals (`--explain`), which is the defensible contract.
- Degraded modes: missing model or failed query embedding → FTS-only;
  memory without embedding → keyword side only.

## Consequences

- Ranking is a pure function of (query, stored data, constants) — pinned
  by determinism tests.
- Measured on the eval corpus: keyword queries — FTS and hybrid both
  Recall@5 1.00 / MRR 1.00 (no regression); paraphrase queries with zero
  keyword overlap — FTS 0.00 by construction, hybrid Recall@5 1.00,
  MRR 0.91.

## Alternatives considered

- **Score normalization (min-max, z-score)** — rejected: unstable across
  result sets, hard to explain.
- **Learned re-ranker (cross-encoder)** — rejected: heavy, opaque,
  Phase 6+ material at best.
