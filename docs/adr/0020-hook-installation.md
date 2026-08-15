# ADR-0020 — Hook installation & preservation

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

`recall git install` and `recall shell install` modify files the user
already owns (`.git/hooks/post-commit`, the shell startup file).
Installation must be explicit, reversible, idempotent, and must never
destroy existing user content. Repositories come in worktrees, with
separated git dirs, and as bare repos — no `.git/hooks` path may be
assumed.

## Decision

- **Marked blocks.** Every file Recall writes is delimited by marker
  comments (`# >>> recall <integration> >>>` / `# <<< ... <<<`).
  Install = append the marked block (idempotent: already-present means
  no-op). Uninstall = remove exactly the marked block; if nothing but a
  shebang remains, delete the file; user content outside the markers is
  never touched.
- **Hook location resolved through git, not assumed:** `git rev-parse
  --git-path hooks` — correct for worktrees and separated git dirs
  (worktree installs land in the *common* hooks dir, pinned by a test).
  Bare repositories: installation refused with a clear message (no
  working tree, no commit flow, no hook).
- **Existing user hooks are never overwritten.** `recall git install`
  refuses when a post-commit hook exists without a recall block and
  prints the options. `--append` adds the recall block *after* the
  user's content; uninstall then removes only the recall block,
  restoring the user's hook byte-for-byte.
- **CLI surface:** `recall git install [--append]`, `recall git
  uninstall`, `recall git status`; `recall shell init | install |
  uninstall | status [--shell powershell|bash|zsh]`. Installation is
  always a deliberate command — Recall never installs itself.
- **Failure behavior:** a broken (partial) marked block is reported as
  PARTIAL by status and refused by install, never silently rewritten.

## Consequences

- Reversibility is testable and tested: install → uninstall cycles
  restore the exact prior state (user-hook preservation, recall-only
  file deletion, append/uninstall round-trips).
- The PowerShell startup-file path comes from PowerShell itself
  (`$PROFILE` via `pwsh`/`powershell -NoProfile`), not a reimplemented
  rule — correct across PowerShell editions.
- No changes to `core.hooksPath`: Recall never hijacks the user's hook
  directory strategy.

## Alternatives considered

- **`core.hooksPath` takeover** — rejected: hijacks *all* hooks and
  would require replicating every existing user hook to preserve them.
- **Overwrite-on-install with backup files** — rejected: backup
  recovery is fragile and the backup itself can be committed by
  accident; refusing (with `--append` as the explicit escape hatch) is
  simpler and safer.
- **A wrapper hook that chains the user's hook** — rejected: rewriting
  the user's script is what preservation exists to avoid.
- **Registering installs in a global manifest** — rejected: state
  outside the target file drifts; the marked block is its own manifest.

## Revisit conditions

If a hook manager ecosystem becomes relevant to real usage, verify
coexistence (marked-block removal is already manager-friendly since it
touches only its own lines).
