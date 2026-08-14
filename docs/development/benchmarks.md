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
