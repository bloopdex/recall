# ADR-0001 — Rust project structure

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

The BloopLab repo standard separates documentation, implementation,
fixtures, scripts, and CI. The Rust crate must have clean boundaries
without enterprise-style ceremony.

## Decision

Repo layout: `docs/`, `codebase/recall/` (the crate), `scripts/`,
`fixtures/`, `.github/workflows/`. Inside the crate: `src/main.rs` (thin
binary) + `src/lib.rs`, with `cli/`, `application/`, `domain/`,
`infrastructure/` (database, git), `config/`, `error/`, `observability/`.

## Consequences

- The library/binary split lets integration tests and the bench example use
  the same code paths as the CLI.
- Dependency direction is one-way: application → domain, application →
  infrastructure; nothing depends upward.

## Alternatives considered

- **Single main.rs crate** — rejected: no way to test the library directly.
- **Workspace with many crates** — rejected: premature; one crate with
  clear modules is enough at this scale.
