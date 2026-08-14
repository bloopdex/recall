# ADR-0005 — FTS5 search design

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Phase 1/2 keyword search must be a real SQLite FTS5 index — not a custom
search engine — with useful ranking and injection-proof queries.

## Decision

- **External-content FTS5** (`content='memories'`) with insert/update/
  delete triggers — the canonical table stays the single source of truth.
- **Tokenizer:** `unicode61 remove_diacritics 1`.
- **Query normalization:** split input into whitespace terms; drop
  non-alphanumeric-only terms; wrap each term in a quoted FTS5 string
  literal (embedded quotes doubled); join with implicit AND. Reject
  all-punctuation queries with a clear error. Injection impossible by
  construction.
- **Ranking:** `bm25(memories_fts, 5.0, 3.0, 5.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0)`
  — problem (5.0) and error (5.0) dominate; solution 3.0; background fields
  1.0. Lower is better; ties break by `captured_at DESC`.

## Consequences

- Exact error-message fragments (quotes, colons, parens) remain searchable.
- Recency acts as a tiebreak, not a rank input (semantic re-ranking with
  recency arrives in Phase 3).

## Alternatives considered

- **Contentless FTS5 + manual sync in code** — rejected: easy to get wrong.
- **Custom inverted index in Rust** — rejected: reinventing SQLite.
- **OR-term matching** — rejected: AND keeps results precise and is
  predictable; revisit if recall (the metric) disappoints.
