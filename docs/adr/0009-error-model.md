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
