# ADR-0007 — CLI architecture

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Phase 0 fixed the surface: `recall capture`, `recall search "<query>"`,
flags `--project/--stdin/--interactive`-style frictionlessness. Future
phases add `edit`, `export`, hooks, and more.

## Decision

clap-derive with subcommands (`capture`, `search`, `list`) and two global
flags (`--db`, `--verbose`). The dispatch core is a 10-line match — new
subcommands plug in without touching existing ones. Capture accepts
optional-field flags (`--error`, `--context`, `--investigation`,
`--root-cause`, `--verification`, `--environment`, `--explanation`) —
optional information supported without a questionnaire.

Exit codes: 0 success (including "no results"), 1 runtime/validation
error, clap's 2 for usage errors.

## Consequences

- Scriptable capture: every field settable by flag.
- The shell-integration phase (4) can wrap the same subcommands.

## Alternatives considered

- **Free-form argument parsing** — rejected: no help, no future-proofing.
- **Interactive TUI (dialoguer etc.)** — rejected: dependency + friction;
  plain prompts match the Phase 0 UX.
