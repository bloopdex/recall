//! Observability — lightweight structured logging for a local CLI.
//!
//! JSON-able structured events via `tracing`, no external sinks, no
//! telemetry, nothing ever leaves the machine. Named metrics
//! (`captures_count`, `search_duration_ms`) are emitted as structured
//! fields on the corresponding events.
//!
//! Policy:
//!
//! - **Logs carry ids, counts, and metadata — never memory content or
//!   query terms.** The `#[instrument]` spans on capture/edit/search
//!   explicitly skip their content-carrying arguments (they previously
//!   leaked raw problem/solution text into `--verbose` output), and the
//!   `search.run` event does not log the raw query. Pinned by
//!   `tests/cli_hardening.rs::logs_never_carry_memory_content_or_secrets`.
//! - **Per-operation metrics only.** `captures_count`, `search_count`,
//!   and `search_duration_ms` exist as structured fields on the
//!   capture/search events. Cumulative cross-process counters were
//!   researched and rejected: Recall is a one-shot CLI with no daemon,
//!   so a running total would need its own store. Consumers (such as a
//!   future DeployScore feed, ADR-0029) will use these per-operation
//!   events instead.
//! - Logs go to STDERR only (stdout is reserved for data).

use tracing_subscriber::EnvFilter;

/// Initialize the tracing subscriber.
///
/// Levels: `RECALL_LOG` env var when set (explicit override), otherwise
/// `recall=warn` — the default is quiet so everyday commands print no
/// log noise; `--verbose` raises it to `recall=debug` (structured
/// events, ids/counts only, never memory content).
pub fn init(verbose: bool) {
    let filter = if let Ok(level) = std::env::var("RECALL_LOG") {
        EnvFilter::new(level)
    } else if verbose {
        EnvFilter::new("recall=debug")
    } else {
        EnvFilter::new("recall=warn")
    };
    // Logs go to STDERR: stdout is reserved for data (search results,
    // `recall export` JSON) and must never be polluted by log lines.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
