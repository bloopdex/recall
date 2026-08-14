# Recall — Benchmarks

Repeatable baseline via `cargo run --release --example bench_search`
(or `scripts/benchmark.ps1`): seeds a deterministic 10,000-entry dataset
and measures FTS5 keyword-search latency (avg/min/max over 20 runs per
query).

## Phase 6 target

Keyword search across **10,000 entries in <100ms** (baseline first — the
Phase 1/2 numbers below are the reference point).

## Baseline — Phase 1/2 (recorded 2026-08-14)

| Date | Machine | Entries | Query | Avg | Min | Max | Hits |
|---|---|---|---|---|---|---|---|
| 2026-08-14 | Windows 11, 16 logical CPUs, release build | 10,000 | postgres connection pool | 3.75 ms | 3.41 | 4.25 | 20 |
| 2026-08-14 | Windows 11, 16 logical CPUs, release build | 10,000 | database is locked | 3.17 ms | 2.94 | 3.49 | 20 |
| 2026-08-14 | Windows 11, 16 logical CPUs, release build | 10,000 | tls handshake timeout | 3.26 ms | 2.97 | 3.49 | 20 |
| 2026-08-14 | Windows 11, 16 logical CPUs, release build | 10,000 | kafka consumer lag | 3.17 ms | 3.02 | 3.43 | 20 |
| 2026-08-14 | Windows 11, 16 logical CPUs, release build | 10,000 | ci pipeline flake | 2.94 ms | 2.67 | 3.30 | 20 |
| 2026-08-14 | Windows 11, 16 logical CPUs, release build | 10,000 | migration missing column | 3.25 ms | 3.05 | 3.62 | 20 |

Seed throughput: 10,000 inserts in ~1.7 s (~5,900 inserts/s, transactional
with FTS triggers active).

**Conclusion:** the Phase 1/2 baseline (~3 ms) already sits ~30× under the
Phase 6 target (<100 ms). Re-measure after Phase 3 adds semantic
re-ranking; the target is comfortably achievable.

## Capture latency — Phase 2 baseline (recorded 2026-08-14)

Two measurements of the actual capture operation (git detection with
branch + commit + dirty file, dedup check, transactional insert), Windows
11 / 16 logical CPUs, release build:

| Measurement | Iterations | Avg | Min | Max | Median |
|---|---|---|---|---|---|
| In-process (`examples/bench_capture.rs`) | 50 | **99.3 ms** | 91.3 | 144.5 | 98.7 |
| Spawned binary (`tests/bench_capture.rs`) | 30 | **117.3 ms** | 111.0 | 130.5 | 116.2 |

Phase 2 target: <100 ms per capture with git context.

**Verdict:** the in-process capture operation sits *at* the 100 ms target
with no margin (99.3 ms); the end-to-end CLI is above it (117 ms) because
of Windows process startup (~18 ms) and the four git subprocess calls that
dominate the profile. No target claim beyond what is measured: the
operation meets the target marginally on this machine; the CLI does not.

**Optimization leads (Phase 5+):** replace the four git invocations with a
single `git status --porcelain=v2 --branch` call, cache git detection per
directory, and skip the `--show-toplevel` call when the cwd is unchanged.

