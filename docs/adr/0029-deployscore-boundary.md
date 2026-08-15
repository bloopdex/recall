# ADR-0029 — DeployScore integration: deferred, with a defined future boundary

- **Date:** 2026-08-15
- **Status:** Accepted (deferral, with revisit conditions)

## Context

The Phase 7 roadmap and DoD call for Recall to "feed incident tags into
DeployScore". The phase contract explicitly forbids inventing
interfaces: "Do not invent APIs for DeployScore. If an external
interface is unavailable or ambiguous, investigate the existing
project/files first and document the limitation."

## Research (what actually exists)

- **DeployScore is design-only.** Its Logseq page has an empty `repo::`
  property, `current-phase:: Phase 0 - Scoring Model Research`, and zero
  DONE items across all 8 phases. The Source of Truth marks it
  `[PLANNED]`. No DeployScore repository exists anywhere on disk
  (A:\BloopLab contains only Recall and the SOT).
- **No incident-ingestion contract exists.** The one relevant design
  artifact — the architecture page — sketches "Incident (repo, commit
  range, description, tagger, timestamp)" and `POST /v1/events`
  ("webhook receiver for CI events, signature-validated, idempotent per
  event ID"), but both are Phase 3 design TODOs: no payload schema, no
  tag format, no wire contract, no sample input. The Phase 7 testing
  TODO ("test the DeployScore feed output against a sample
  DeployScore-compatible input") has literally nothing to be compatible
  with.
- **The signal contract is two-sided by design.** The SOT (Section 7)
  states: signal flows between tools are contracts; when tool A feeds
  tool B, BOTH sides document the event/report shape — "this is a TODO
  in each relevant Phase 6/Integration phase". DeployScore's side of
  that TODO is Phase 3/4 work that has not happened.

## Decision

**Do not implement a DeployScore feed now.** Any implementation would
have to invent the incident-record format, making Recall the de-facto
spec-setter for a project whose own design phase has not run — exactly
the fabrication the contract forbids.

**What is done instead:**

1. **The future boundary is defined on Recall's side without inventing
   a format:** the existing portable export (ADR-0024) — versioned
   JSON, `format_version`/`recall_schema_version`, project, error,
   captured_at, git_commit — already carries everything an incident
   record needs. A future DeployScore provider (its Phase 4 "Recall
   incident history" signal) can consume `recall export` (redacted by
   default; `--include-secrets` opt-in) or its library equivalent. No
   new wire format, no new network path, no coupling.
2. **The proposal for DeployScore's side is recorded in the Phase 7
   research record and the SOT:** when DeployScore reaches Phase 3/4, it
   defines the incident-record schema; Recall then adds an
   `--incidents`-style projection of the same export data if and only
   if a projection beyond the portable export is warranted. Recall must
   remain fully usable without DeployScore (integration is one-way,
   optional, read-only from Recall's perspective).

## Alternatives considered

- **Invent an "incident tags" JSON format and a `recall export
  --incidents` command now** — rejected: fabricates a contract the
  receiving side has not designed; the test TODO cannot be honestly
  satisfied ("sample DeployScore-compatible input" does not exist).
- **HTTP POST to a DeployScore endpoint** — rejected twice over:
  the endpoint does not exist, and it would break the zero-network
  guarantee for a recipient nobody runs.
- **Defer silently** — rejected: the deferral must be explicit, with
  the boundary and revisit conditions documented (this ADR).

## Consequences

The Phase 7 DoD item "DeployScore receives incident tags from Recall
entries" is explicitly NOT satisfied — cannot be, from the available
project state — and is documented as deferred with the exact reasons.
The honest failure mode is honored: what was investigated, why it could
not be completed, what was completed instead, what remains, and why it
is deferred. Revisit conditions: DeployScore ships Phase 3/4 with a
defined incident-record contract; then Recall adds the projection on
the export boundary, tests against the real fixture, and the signal
contract is recorded on both sides per SOT Section 7.
