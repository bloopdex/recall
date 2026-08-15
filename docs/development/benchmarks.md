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

## Phase 4 — shell & git integration baselines (recorded 2026-08-15)

Machine: Windows 11, 16 logical CPUs, release build
(`scripts/bench_phase4.ps1` + `examples/bench_sanitize.rs`).

**Prompt-hook overhead** (PowerShell, 200 prompt invocations per variant):

| Variant | Avg per invocation |
|---|---|
| Plain `prompt` function (baseline) | 0.644 ms |
| Recall prompt hook (snapshot recording) | 1.119 ms |
| **Overhead** | **0.475 ms** |

Phase 4 budget: the prompt must add <50 ms to the shell path. Measured
overhead is ~100x under budget — the hook is invisible to typing latency.

**`recall capture --from-shell` end-to-end** (spawned release binary,
snapshot env vars set, git context present, 20 runs):

- **118.3 ms/capture** — statistically the same as the Phase 2 plain
  capture baseline (117.3 ms); the snapshot read + sanitization scan
  cost is within noise. Process startup and the four git subprocess
  calls continue to dominate (Phase 2 findings unchanged).

**Git post-commit hook overhead** (10 commits each, non-interactive skip
path):

| Variant | Avg per commit |
|---|---|
| With recall hook installed | 85.3 ms |
| Without hook | 86.8 ms |
| **Overhead** | **≈ 0 ms** (noise-dominated) |

The skip path (`command -v recall` + TTY check) is unmeasurable against
git's own commit-time variance. The reliability boundary costs nothing
in the common case.

**Secret sanitization throughput** (`examples/bench_sanitize.rs`):

- 3.4 KB realistic error log (7 secret shapes), 500 iterations:
  **156 µs/scan ≈ 21 MB/s**. At the 10 KB captured-text limit that is
  ~0.5 ms — negligible against the 118 ms capture.

**Conclusion:** every Phase 4 latency budget is met with orders of
magnitude to spare; no optimization was performed because none is
warranted by the measurements.

## Phase 5 — project-scoped search baselines (recorded 2026-08-15)

Machine: Windows 11, 16 logical CPUs, release build
(`examples/bench_projects.rs`): 10,000 memories + synthetic vectors
across 10 projects, 20 iterations per measurement.

| Engine | Scope | Avg | Min | Max |
|---|---|---|---|---|
| FTS | global | 9.9 ms | 9.0 | 13.8 |
| FTS | project-03 | **7.2 ms** | 6.8 | 7.6 |
| Semantic | global | 18.9 ms | 18.3 | 19.5 |
| Semantic | project-03 | **19.3 ms** | 18.5 | 20.6 |
| Hybrid | global | 29.7 ms | 28.4 | 35.7 |
| Hybrid | project-03 | **27.1 ms** | 25.7 | 31.2 |

Notes:

- Scoped FTS is slightly *faster* than global (fewer rows to order).
- Semantic numbers here are higher than the Phase 3 baseline (~9.8 ms):
  the Phase 5 second lookup joins `memories` for the status/project
  predicates. The join costs ~9 ms and buys one filtered code path for
  every engine — accepted, still ~5× under the <100 ms target.
- Project filtering rides on the existing `project` index at this
  scale; no new index was added (measured first, per the standard).


