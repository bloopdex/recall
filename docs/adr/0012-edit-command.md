# ADR-0012 — `recall edit`: user fields only

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Memories age: solutions get superseded, errors get clarified. Phase 2
requires an edit path that updates both the row and the FTS5 index.

## Decision

- `recall edit <id>` with per-field flags for the nine user-provided
  fields (`--problem`, `--solution`, `--error`, `--context`,
  `--investigation`, `--root-cause`, `--verification`, `--environment`,
  `--explanation`).
- Field semantics: flag absent = untouched; text = replace; empty text =
  clear (optional fields only — clearing `problem`/`solution` is rejected).
- **Automatically captured metadata is not editable** (project, repo path,
  branch, commit, changed files, cwd, timestamps). It is evidence, not
  user content; editing it would falsify the record.
- FTS5 synchronization needs no code: the schema's UPDATE trigger rebuilds
  the index entry in the same transaction.
- Missing id → clear error, exit 1. No flags → clear error, exit 1.

## Consequences

- Search results always reflect edited text immediately.
- The canonical data model is unchanged: no schema change, no new column.

## Alternatives considered

- **Editable everything** — rejected: auto-captured metadata is a
  trustworthy context record; allowing casual edits destroys that.
- **`edit` via a new capture with dedup link** — rejected: more complex
  than a targeted UPDATE and loses the edit history semantics.
