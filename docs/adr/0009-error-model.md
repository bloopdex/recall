# ADR-0009 — Error model

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Errors must be typed and detailed internally, user-readable and actionable
at the CLI boundary, and must never leak internals (raw SQL) or sensitive
content.

## Decision

`thiserror` enum (`Error`) as the crate-wide error type: config, database,
migration, invalid-input, io, git, timestamp, and search variants, each
with an actionable message. The CLI boundary prints `error: {message}` to
stderr and exits 1; `anyhow` is used only as the top-level glue in
`cli::run`.

## Consequences

- One error vocabulary; callers match on variants where behavior differs
  (none needed yet — kept for future phases).
- Migration failures name the migration; validation failures name the
  field; database failures stay user-readable ("file is not a database").

## Alternatives considered

- **anyhow everywhere** — rejected: no typed matching, easy to leak
  context strings.
- **Per-module error enums** — rejected: ceremony without benefit at this
  scale.

## Amendment (Phase 6, 2026-08-15) — the CLI error contract

Phase 6 pinned the exit-code contract in `tests/cli_hardening.rs`:

- `0` — success, including "No results" and first-run database
  creation (a missing database is created on first use by design).
- `1` — runtime error: a typed `Error` printed to stderr, actionable,
  never a panic. Two new variants: `DbCorrupt` (the file is not a
  readable Recall database — the message carries the recovery model:
  restore the pre-migration backup or re-import an export) and
  `CheckFailed` (`recall check` found problems — the report is on
  stdout, the non-zero exit lets scripts gate on it).
- `2` — usage error (clap): missing/invalid arguments, unknown
  subcommands.

Also pinned: hostile filter values are inert (parameterized SQL —
injection-shaped project names match nothing), unicode text
round-trips, very large piped input is captured, and logs never carry
memory content or secrets (the instrument-spans on capture/edit/search
skip their content arguments — a Phase 6 fix; the search.run event no
longer logs the raw query).
