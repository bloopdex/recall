# ADR-0022 — Project-aware search: global by default, explicit scoping

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

Phase 5 requires project-aware search: scoped search, global search, and
a default that minimizes surprises. The original Phase 5 page specified
`--all` as the default.

## Decision

- **Global search is the default.** `recall search "connection pool"`
  searches every active memory, unchanged from Phases 1–4. The canonical
  Recall use case is a fix from *months ago, possibly in another project*
  — scoping by default would hide exactly the memory Recall exists to
  resurface. Every result displays its project, so provenance is never
  ambiguous.
- **Explicit scoping:** `recall search --project <name> <query>` (and the
  same flag on `recall list`) restricts to one project label, matched
  case-insensitively. An unknown label yields "No results", not an error.
  Memories with no project (`NULL`) appear in global search only — there
  is no name to target them with; `recall projects` reports them as
  "(no project)".
- **Filtering is one code path for all three engines** (`SearchFilter`
  in the database layer): FTS via a WHERE on the JOIN, semantic via the
  second lookup over the ≤k MATCH rowids, hybrid by filtering both sides
  before RRF fusion. No second search implementation exists.
- **Documented semantic consequence:** the vec0 MATCH always returns the
  unfiltered top-k; project/status filtering happens afterwards, so a
  small project may surface fewer than k semantic hits. Fine at personal
  scale; pinned by tests.
- `recall projects` (labels, counts, last capture, current project
  marked) derives from a GROUP BY — no registry (ADR-0021).

## Alternatives considered

- **Current-project default** — rejected: hides cross-project memories;
  the canonical Phase 0 example (a fix found months later) spans
  projects.
- **A separate search engine per filter** — rejected: duplicated ranking
  logic, drift risk; the single filtered pipeline keeps Phase 3's
  deterministic RRF identical for every scope.

## Consequences

Measured (10k memories across 10 projects, release build): scoped FTS
~7.2 ms (vs ~9.9 ms global — fewer rows to order), semantic ~19.3 ms
(vs ~18.9 ms global — the added status/project predicate on the second
lookup is within noise), hybrid ~27.1 ms (vs ~29.7 ms). Filtering costs
nothing measurable; all numbers stay ~4–5× under the Phase 6 <100 ms
target.
