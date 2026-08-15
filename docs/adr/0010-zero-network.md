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

## Amendment (2026-08-14, ADR-0013)

The embedding stack (fastembed/ONNX Runtime) and the one-time model
download introduce network crates in two distinct ways:

1. **Direct:** exactly one — **reqwest**, compiled only behind the
   default-off `download` feature and used exclusively by
   `recall embeddings download`. The Cargo.toml guard test enforces the
   carve-out precisely: reqwest + `optional = true` is the only allowed
   banned-name combination.
2. **Transitive:** fastembed itself depends on reqwest/hyper/tokio for
   its hf-hub download path — a code path Recall never calls (the model
   is loaded from a local user-defined directory). The tree check
   (`scripts/check_no_network.ps1`) therefore allows banned crates only
   when `cargo tree -i` shows fastembed as the parent, and a new
   source-guard test pins that Recall's own modules never reference
   network APIs (the only sanctioned exception is the feature-gated
   download module).

The runtime guarantee — Recall never contacts the network — holds:
every network-capable code path in the linked binary is dead code from
Recall's perspective, and model presence is verified before any embedder
construction.

## Amendment 2 (2026-08-15, ADR-0017)

The Phase 4 shell integration transports the failure snapshot through
environment variables (`RECALL_LAST_COMMAND`, `RECALL_LAST_EXIT_CODE`,
`RECALL_LAST_CWD`), which requires reading three named variables — a
narrow, documented exception to the "capture reads no environment"
construction rule. The rule is amended, not weakened:

- `application/capture.rs` still reads **no** process environment (the
  original pin test remains).
- The only module allowed to read environment variables is
  `infrastructure/shell.rs`, and only the whitelist:
  `RECALL_LAST_COMMAND`, `RECALL_LAST_EXIT_CODE`, `RECALL_LAST_CWD`,
  `HOME`, `USERPROFILE`, `SHELL` — pinned by a dedicated security test.
- The snapshot variables are written by Recall's own prompt hook, never
  enumerated: `std::env::vars()` remains absent from the codebase, and
  no environment content can enter a memory beyond the whitelist.

## Amendment 3 (Phase 6, 2026-08-15) — enforcement moved into CI

The zero-network guarantee is now checked three ways, and the strongest
one runs on every platform in CI automatically:

1. The manifest test (direct dependencies only).
2. The source-reference test (Recall's own code).
3. **New: a full `cargo tree` scan** — `tests/security.rs` walks the
   entire transitive dependency tree for both the default build and the
   opt-in `download` build, and fails if any banned network crate is
   reachable outside the sanctioned paths (under `fastembed`, or under
   `reqwest` when the `download` feature is on). This is the same rule
   as `scripts/check_no_network.ps1`, now inside `cargo test` — so
   GitHub Actions enforces it on ubuntu and windows with zero extra CI
   steps. (Notable finding: `reqwest` itself is pulled transitively by
   hf-hub under fastembed — the carve-out covers it, and the manifest
   test still pins that Recall's own optional dependency stays
   feature-gated and default-off.)
