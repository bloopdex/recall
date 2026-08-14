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

## Phase 3 — semantic layer baselines (recorded 2026-08-14)

Machine: Windows 11, 16 logical CPUs, release build, all-MiniLM-L6-v2
fp32 ONNX on CPU.

**Embedding generation** (`examples/bench_embed.rs`):

| Measurement | Iterations | Avg | Min | Max |
|---|---|---|---|---|
| Model load (per CLI invocation) | 1 | **163 ms** | — | — |
| Single text embedding | 50 | **4.6 ms** | 1.7 | 30.9 |
| Batch of 32 texts | 20 | **38.1 ms** | 14.5 | 77.9 |

**Search latency at 10,000 entries + 10,000 vectors**
(`examples/bench_search.rs`, synthetic deterministic vectors — latency is
data-driven; quality is measured by the eval harness):

| Engine | Query | Avg | Min | Max |
|---|---|---|---|---|
| FTS5 (keyword) | all six | ~3.1–3.8 ms | ~2.7 | ~5.3 |
| Semantic (vec0 k=50) | all three | **~9.8 ms** | 9.4 | 10.5 |
| Hybrid (RRF) | all three | **~14.5 ms** | 13.8 | 16.5 |

Vector seed throughput: 10,000 vectors in ~1.3 s.

Phase 3 target ("vector queries must stay interactive on 10k entries"):
met with large margin — semantic and hybrid stay far below 100 ms.

**Search quality** (`examples/eval_search.rs`, 24-memory corpus, 12
queries, Recall@5 / Precision@5 / MRR):

| Segment | FTS R@5 | FTS MRR | Hybrid R@5 | Hybrid MRR |
|---|---|---|---|---|
| Keyword queries (regression check) | 1.00 | 1.00 | 1.00 | 1.00 |
| Paraphrase queries (zero keyword overlap) | 0.00 | 0.00 | **1.00** | **0.91** |

Hybrid matches or exceeds FTS everywhere; on paraphrase queries it finds
every target (7 of 8 at rank 1), where keyword search is structurally
unable to. Re-run after any ranking/model change.

