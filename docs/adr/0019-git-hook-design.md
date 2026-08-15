# ADR-0019 — Git integration: non-blocking post-commit hook with an explicit gate

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

The second Phase 4 workflow: a problem is fixed, the developer commits,
and Recall offers to capture the fix. The hard reliability boundary is
that Git must never depend on Recall: if Recall is missing, slow, or
broken, commits proceed exactly as before.

## Decision

- **Hook: `post-commit`, and only `post-commit`.** It fires after the
  commit succeeded, so the reliability boundary is *structural*: a
  post-commit hook cannot abort the commit it follows. The hook script
  is a POSIX sh wrapper:
  `if command -v recall >/dev/null 2>&1; then recall capture --from-git || true; fi`
  — Recall missing → skip; Recall fails → `|| true`; Recall slow → the
  only cost is post-commit wall time, and the hook skips instantly when
  stdin is not a terminal.
- **No fix-detection heuristics.** The original Phase 4 TODO proposed
  classifying commits by message patterns ("fix", "resolve", issue
  numbers). Research conclusion: message heuristics produce silent
  false negatives (a legitimate fix committed as "tweak pool" is never
  offered) and false positives (chore commits triggering prompts) — and
  the human at the keyboard is a strictly better classifier than a
  regex. The gate is therefore **explicit**: `recall capture --from-git`
  pre-fills the problem from the commit subject and asks; pressing
  Enter captures, typing `skip` declines, empty solution never passes
  Phase 2 validation. No commit is captured without the user deciding.
- **Non-interactive contexts** (CI, GUI git clients, scripts): stdin is
  not a terminal and no explicit `--problem`/`--solution` was given →
  capture skips immediately with a one-line message, exit 0.
- **Pre-fill contents:** problem = `Fix in {project}: {commit subject}`;
  `git_commit` and the commit's changed files (`git show --name-only
  HEAD`, capped at 50 lines like ADR-0008) replace the working-tree
  snapshot in `--from-git` mode.

## Consequences

- `git commit` behaves identically with or without Recall — pinned by a
  test that commits with the hook installed and Recall removed from
  PATH.
- Captures happen at the natural moment (just after the fix exists in
  history) with full git context, while the *decision* to capture stays
  human.
- Automatic capture of every commit is explicitly out of scope: the
  gate is per-commit and explicit.

## Alternatives considered

- **pre-commit hook** — rejected: fires before the fix is committed;
  prompts block the commit itself (violates the reliability boundary in
  the wrong direction).
- **commit-msg / prepare-commit-msg hooks** — rejected: those hooks
  exist to shape the message, not to capture after success.
- **Automatic capture with message heuristics** — rejected: silent
  misclassification; see above.
- **Background capture daemon watching the reflog** — rejected:
  contradicts the single-binary architecture and adds a permanent
  process for a per-commit decision a human makes better.

## Revisit conditions

If hook prompts prove annoying in real use, add a per-repo opt-out
(`git config recall.enabled false`, read by the hook) — deliberately
not built until the annoyance is observed, not predicted.
