# ADR-0025 — Phase 6 performance strategy: measure at scale, optimize on evidence

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

The project performance target is "search across 10k entries in <100 ms"
(Phase 1 baseline, BloopLab performance standard: baseline → measure →
optimize → benchmark → document). Phase 1–5 recorded avg/min/max numbers
at 10k only. Phase 6's job is to determine whether the system stays
performant as data grows (10k → 50k → 100k), across projects, archives,
and with/without embeddings — and to fix only what measurement shows to
be broken.

## Decision

- **One harness, every operation:** `examples/bench_phase6.rs` measures
  FTS / semantic / hybrid (global, scoped, include-archived), list,
  edit, archive, delete, export, import, and the capture flow at
  10k/50k/100k entries, 10 projects, ~10% archived. `EMBED=0` runs the
  no-embedding axis. Deterministic XorShift dataset (the house pattern —
  no `rand` dependency), synthetic vectors for search latency.
- **Percentiles, not averages:** every cell reports avg, median, p95,
  p99, min, max (nearest-rank). Averages alone hid nothing in practice,
  but p95/p99 is what a user experiences on the slow tail.
- **Startup separated from work:** application-layer ops are timed
  in-process; process startup is measured by spawning the release binary.
  Search rows exclude model inference (a separate concern — model
  throughput is `bench_embed`'s job; quality is the eval harness's).
- **No optimization without evidence.** The audit found nothing to
  optimize: the Phase 6 bottleneck hunt concluded the system is database-
  bound in the expected places (full vec0 scans for semantic search) and
  comfortable against the target. The one measured overshoot (in-process
  capture ≈103 ms average vs the 100 ms Phase 2 target) is dominated by
  git subprocess spawns (~85 ms of it); the breakdown is measured and
  documented, and the fix is deferred: capture is dominated by `git`
  process spawn cost, and batching git calls is a change with its own
  reliability trade-offs, not a Phase 6 bug. (Phase 2's target was
  defined for the capture OPERATION; the full CLI path including startup
  was never the measured unit.)
- **Benchmarks stay reproducible:** committed script, fixed seeds,
  machine/OS/CPU recorded in `docs/development/benchmarks.md` alongside
  every run. CI does NOT run benchmarks — shared runners are too noisy
  to gate latency on; the recorded local numbers are the regression
  reference.

## Alternatives considered

- **criterion in `benches/`** — rejected: adds a dependency tree to the
  zero-network surface; the hand-rolled harness reports exactly the
  metrics the phase needs (p95/p99 across a dozen operations) and runs
  as an example without any new crate.
- **Benchmarks as CI gates** — rejected: flaky on shared runners;
  performance regression is checked locally against the documented
  reference runs.
- **Optimizing semantic search with ANN indexes (e.g. vec0's auxiliary
  index options)** — not done: measurements at 100k are still far under
  target; premature optimization violates the phase's own principle.

## Consequences

The <100 ms target is verified or its limitation documented at every
scale in one place. Future phases re-run the harness before and after
any storage-layer change; a regression is visible as a percentile shift,
not a gut feeling.
