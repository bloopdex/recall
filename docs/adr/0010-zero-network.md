# ADR-0010 — Zero-network enforcement

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

ADR-0000 makes privacy structural. "No network calls" must be enforceable,
not aspirational.

## Decision

Three layers of enforcement:

1. **Dependency guard:** `tests/security.rs` fails if any networking /
   async-runtime crate name appears in `Cargo.toml` (reqwest, hyper, tokio,
   ureq, isahc, curl, openssl, rustls, native-tls, surf, attohttpc, minreq,
   websocket, async-std, smol).
2. **Tree guard:** `scripts/check_no_network.ps1` runs the same check
   against `cargo tree` (catches transitive pulls).
3. **Construction:** the capture path reads no process environment
   (`tests/security.rs` pins that); the only external process spawned is
   `git`, with fixed arguments.

## Consequences

- The binary has no code path that can reach the network.
- Adding telemetry or sync would require knowingly breaking two tests and
  an ADR.

## Alternatives considered

- **Runtime sandboxing** — rejected: platform-specific, disproportionate.
- **Code review only** — rejected: not enforceable.

## Revisit conditions

If a future phase ever justifies network access (not planned), this ADR
must be superseded explicitly and the privacy position re-evaluated.
