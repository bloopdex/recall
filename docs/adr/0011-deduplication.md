# ADR-0011 — Deduplication: deterministic skip, not merge

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Capture friction is the make-or-break of a memory tool, but so is store
hygiene: re-capturing the same incident twice pollutes search results.
The Phase 2 planning page proposed a "merge-or-skip prompt" for
near-identical captures.

## Decision

- **Near-identical rule:** two memories are near-identical when they share
  the same project (both NULL counts as shared) and, within a 30-day
  window, share either the normalized problem text or the normalized
  error text (lowercase, whitespace-collapsed).
- **Behavior:** the capture is **skipped deterministically** — no prompt,
  no merge. The CLI prints which existing memory matched and how to
  override; `--force` captures anyway.
- Determinism first: the same inputs always produce the same decision.
  The rule is pure string comparison — no scoring, no heuristics that
  drift with data.

## Consequences

- Frictionless: no interactive question in the middle of capture; piped
  and CI captures behave identically to interactive ones.
- A genuinely new solution for an old error is preserved via `--force`.
- **Supersedes** the Phase 2 page's "merge-or-skip" wording: merging two
  memory texts without user judgment would silently corrupt the record;
  the page is updated to match this decision.

## Alternatives considered

- **Interactive merge-or-skip prompt** — rejected: breaks piped capture,
  and merging semantics were never defined.
- **Silent rejection (error exit)** — rejected: an exit code makes
  scripting worse for a situation that is usually harmless.
- **Always capture + de-dupe at search time** — rejected: moves the
  problem into ranking, where it is harder to reason about.

## Why this choice / Trade-offs / Revisit conditions

- **Why:** skip is the only deterministic, prompt-free behavior that keeps
  the store clean without inventing merge semantics.
- **Trade-offs:** a deliberate re-capture needs an extra flag.
- **Revisit when:** the window or the rule needs tuning after real usage,
  or when edit/dedup feedback shows a strong need for merge.
