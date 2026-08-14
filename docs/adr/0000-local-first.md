# ADR-0000 — Local-first architecture

- **Date:** 2026-08-13
- **Status:** Accepted

## Context

Recall stores personal engineering memories — errors, commands, file paths,
solutions. That data is sensitive. The Phase 0 privacy design requires that
nothing leaves the machine.

## Decision

Recall is local-first: one SQLite database on disk, no server, no network
code, no telemetry, no analytics. Export (portable JSON) is opt-in and
arrives in Phase 5. Zero network calls is a test-enforced invariant.

## Consequences

- Privacy is structural, not a policy.
- Distribution is a single binary; no infra to run.
- Cross-machine sync is explicitly out of scope (revisit if ever needed —
  a self-hosted export/import flow, never a cloud service).

## Alternatives considered

- **Sync via a cloud service** — rejected: violates the privacy position.
- **Encrypted cloud backup** — deferred: complexity with no Phase 1/2 need.

## Problem / Options / Why / Trade-offs / Revisit conditions

- **Problem:** where does personal solution history live without leaking it?
- **Options:** cloud DB, local DB + sync, local-only.
- **Why:** local-only is the only option that makes privacy a property of
  the architecture instead of a promise.
- **Trade-offs:** no access from other machines.
- **Revisit when:** multi-machine access becomes a real personal need.
