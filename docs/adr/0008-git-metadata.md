# ADR-0008 — Git metadata strategy

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Capture should record project identity, branch, commit, and changed files
automatically — but git must never be a requirement (capture works outside
repos, with git missing, in empty repos, on detached HEAD).

## Decision

Spawn the `git` executable with fixed argument vectors (`rev-parse
--show-toplevel`, `rev-parse --abbrev-ref HEAD`, `rev-parse --short HEAD`,
`status --porcelain`) — never a shell, never user-provided strings. Every
failure degrades to "metadata absent"; nothing propagates as an error.
`git status` output is capped at 50 lines.

## Consequences

- No `libgit2` C dependency; respects the user's git config and aliases.
- Detached HEAD → branch `None`, commit still captured. Empty repo → commit
  `None`. Missing git → all `None`.
- Best-effort metadata, never a capture requirement.

## Alternatives considered

- **git2 crate** — rejected: heavy native dependency for three read-only
  lookups.
- **Requiring git context** — rejected: violates capture-first.

## Important interpretation

"Automatic git commit" in the Phase 0/2 requirements means *capturing* the
current commit associated with the memory — Recall never creates commits in
the user's projects.
