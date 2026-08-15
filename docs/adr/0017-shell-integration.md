# ADR-0017 — Shell integration: prompt-hook observation, never a command proxy

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

Phase 4 puts capture at the point of failure: a command fails, and
`recall capture --from-shell` should already know what failed and how.
The shell must therefore observe failed commands — but Phase 4's own
principles forbid the integration from becoming a transparent command
proxy: the user's `command` must behave exactly as before (same stdout,
stderr, exit code, env), with no added latency.

## Decision

**Observation, not interception.** Recall integrates at the shell's
*prompt hook* — code the shell itself runs after every command:

- **PowerShell:** a `prompt` function that wraps any existing `prompt`
  (saved as `__recall_original_prompt`). On success (`$?` true) it clears
  the snapshot; on failure it records `$LASTEXITCODE`, the last command
  line from `Get-History`, and the current directory.
- **Bash:** `PROMPT_COMMAND` chaining — Recall's hook is prepended so
  `$?` is captured before anything else runs; the existing
  `PROMPT_COMMAND` is preserved.
- **Zsh:** `precmd_functions` — generated on the same mechanism, marked
  untested (see below).

The hook records the snapshot into **three environment variables** —
`RECALL_LAST_COMMAND`, `RECALL_LAST_EXIT_CODE`, `RECALL_LAST_CWD` — which
`recall capture --from-shell` reads. Env vars were chosen over a state
file: no temp files, no staleness/concurrency, one mechanism across all
shells.

**Output capture is deliberately NOT retroactive.** Terminal output of a
failed command cannot be retrieved after the fact without either
wrapping every command (rejected: changes command behavior and exit-code
semantics) or running a transcript (rejected: slows every command and
captures everything — a privacy hazard). Error text therefore arrives
one of three explicit ways: piped into `recall capture --from-shell`
(stdin becomes the error field in context modes), `--error`, or pasted.

## Consequences

- The user's commands are untouched: no wrapper, no exit-code changes,
  no output changes. The only shell-visible change is three exported
  variables.
- Failure snapshots are explicit data the user can inspect before
  capture (`--from-shell` shows the prefill before asking for the
  solution).
- Supported shells: PowerShell (tested), Bash (tested via Git Bash on
  Windows), Zsh (generated, same mechanism as Bash — untested). CMD is
  **not supported**: its only per-command hook mechanisms (autorun
  registry, doskey macros) cannot capture per-invocation exit codes
  without heavy intrusion; documented as a non-goal.
- The `recall` shell function dispatches known subcommands to the binary
  and routes anything else to `recall search` — on-demand surfacing.
  It runs only when explicitly invoked; per-command pre-emptive
  surfacing remains future work.

## Alternatives considered

- **Transparent command proxy (prompt wrapper that runs every command
  through Recall)** — rejected: changes stdout/stderr/exit-code
  semantics of every command, breaks pipes and subshells, adds
  per-command latency. The exact behavior the principles forbid.
- **bash DEBUG trap** — rejected: fires *before* every command, can't
  see the exit code, adds per-command overhead and reentrancy hazards.
- **Start-Transcript output capture** — rejected: slows every command,
  captures everything indiscriminately (secrets included), no
  redaction boundary.
- **External watcher daemon** — rejected: a background process
  contradicts the single-binary CLI architecture.
- **PSReadLine key handlers** — rejected: interactive-foreground only,
  no coverage of non-interactive flows, extra configuration surface.

## Revisit conditions

If output capture ever becomes a hard requirement, revisit with an
opt-in `recall shell capture-output` transcript mode rather than
default-on wrapping. If Zsh matters to real usage, test the generated
precmd snippet and promote it to "tested".
