# ADR-0013 — Embedding model: all-MiniLM-L6-v2 via fastembed

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Phase 3 needs local, offline semantic embeddings. Candidates were
evaluated against Recall's philosophy: local-first, zero network at
runtime, deterministic where possible, small dependency footprint.

## Decision

- **Model:** sentence-transformers `all-MiniLM-L6-v2`, fp32 ONNX export
  (90 MB), 384 dimensions, Apache-2.0 license, CPU-only inference —
  GPU support is unnecessary at these text sizes (measured ~4.6 ms per
  text on this machine).
- **Runtime:** `fastembed` 4.9 (ONNX Runtime under the hood) loading the
  model **from a user-defined local directory** — Recall controls every
  file read and never calls fastembed's download path.
- **Model acquisition:** the explicit, opt-in `recall embeddings download`
  command fetches the four model files into
  `%LOCALAPPDATA%\recall\models\all-MiniLM-L6-v2\`. The HTTP client
  (reqwest) is compiled ONLY behind the default-off `download` feature;
  default builds contain no network code (ADR-0010 carve-out, verified by
  the security tests).
- **Embedding input:** `problem` + `error` + `context` (present fields,
  newline-joined). The solution is deliberately excluded: the retrieval
  question is "have I solved something like this before?" and queries
  resemble symptom descriptions, not fix wording. Timestamps, git
  metadata, and paths are never embedded.
- **Pooling:** `Pooling::Mean` — verified empirically (pooling=None makes
  unrelated docs nearly as similar as related ones: 0.56 vs 0.59; Mean
  separates 0.39 vs 0.11). Pinned by `tests/pooling_probe.rs`.

## Consequences

- Runtime is fully offline: presence-check before load, clear error
  otherwise. Capture degrades gracefully when the model is absent.
- Model load costs ~163 ms per CLI invocation (measured) — acceptable for
  Phase 3; a daemon/cache is a Phase 5+ optimization.
- Model updates bump `MODEL_VERSION`; stale vectors are identified and
  rebuilt by `recall embeddings build` (ADR-0015).

## Alternatives considered

- **candle (pure Rust)** — rejected: heavier integration effort for the
  same capability; fastembed/ONNX Runtime is the boring, well-trodden path.
- **larger models (bge-base, MiniLM-L12)** — rejected: quality gain does
  not justify 2–5× size/latency for a personal memory tool; revisit in
  Phase 6 tuning.
- **hosted embedding APIs** — rejected outright: violates local-first.

## Revisit conditions

Phase 6 search-quality tuning; if eval metrics regress or a materially
better small model appears.
