# Recall — Design

Design-level notes and rationale behind the capture/search UX.

## Capture UX

```
$ recall capture
Problem:
> sqlite database is locked
Solution:
> set busy_timeout to 5000ms
Captured #1 (project: recall)
```

- **Frictionless by default:** two prompts, nothing else. Optional fields
  are flags (`--error`, `--context`, `--investigation`, `--root-cause`,
  `--verification`, `--environment`, `--explanation`), never a questionnaire.
- **Piped stdin** becomes the problem automatically (no flag needed);
  `--stdin` is the explicit form. Solution then comes from `--solution`.
- **Capture first, enrich later:** git/project/context failures never block
  a capture.

## Search UX

`recall search "postgres connection pool"` returns ranked results showing
rank, capture time (local), project, commit, error line, problem, and
solution — enough to judge usefulness at a glance. No results: a clear
one-line message, exit code 0.

## Privacy position

Recall is engineering incident memory — **not** a generic knowledge base,
not a team wiki, not cloud-synced. Everything stays on disk; export is
opt-in. Auto-capture never collects environment variables; user-typed
fields may contain secrets and the redaction layer treats
`error`/`context`/`investigation` as redactable by contract.

## Semantic search

- `recall search "<query>"` is hybrid: FTS5 candidates + vec0 candidates
  fused by reciprocal-rank fusion (ADR-0016), deterministic and
  explainable; `--explain` shows per-engine signals. No fake confidence
  percentages — measurable signals only.
- Embedding input is problem + error + context (the "what happened" side);
  the solution is deliberately excluded (queries resemble symptoms, not
  fixes). Capture embeds best-effort; edits to embedded fields regenerate
  or invalidate (ADR-0013/0015).
- The eval corpus (`examples/eval_search.rs`) measures quality:
  hybrid Recall@5 1.00 on paraphrase queries where FTS is structurally 0.

## Store hygiene

- **Deduplication (ADR-0011):** near-identical = same project + same
  normalized problem or error, within 30 days → deterministic skip with a
  clear message; `--force` captures anyway. Merge was deliberately not
  implemented — merging texts without judgment would corrupt the record.
- **Edit (ADR-0012):** `recall edit <id>` updates user-provided fields;
  empty text clears an optional field; auto-captured metadata is not
  editable.

## Deliberate non-features

- No fuzzy/typo-tolerant matching (a possible future direction)
- No automatic retention — archive/delete are explicit actions
  (ADR-0023)
- No confidence percentages — measurable, explainable ranking signals
  only (ADR-0016)
