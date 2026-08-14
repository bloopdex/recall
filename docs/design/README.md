# Recall — Design

Design-level notes that connect the Phase 0 research (Logseq:
`Recall / Phase 0 - Data Model & UX Research`) to the implementation.

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
one-line message, exit code 0. Semantic ranking and confidence scores are
Phase 3.

## Privacy position

Recall is engineering incident memory — **not** a generic knowledge base,
not a team wiki, not cloud-synced. Everything stays on disk; export is
opt-in (Phase 5). Auto-capture never collects environment variables;
user-typed fields may contain secrets and Phase 6 redaction treats
`error`/`context`/`investigation` as redactable by contract.

## Deliberate Phase 1/2 scope cuts

- No dedup, no `recall edit` (remain Phase 2 follow-ups)
- No fuzzy matching, no vector search (Phase 3)
- No shell hooks / git hooks (Phase 4)
- No retention policies (Phase 5)
