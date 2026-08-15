//! Observability — lightweight structured logging for a local CLI.
//!
//! Phase 1/2 approach per the BloopLab standard: JSON-able structured events
//! via `tracing`, no external sinks, no telemetry, nothing ever leaves the
//! machine. Named metrics (`captures_count`, `search_duration_ms`) are
//! emitted as structured fields on the corresponding events.
//!
//! Phase 6 review (documented in the Phase 6 research record):
//!
//! - **Log-data policy: logs carry ids, counts, and metadata — never
//!   memory content or query terms.** The `#[instrument]` spans on
//!   capture/edit/search explicitly skip their content-carrying
//!   arguments (a Phase 6 fix — they previously leaked raw problem/
//!   solution text into `--verbose` output), and the `search.run` event
//!   does not log the raw query. Pinned by
//!   `tests/cli_hardening.rs::logs_never_carry_memory_content_or_secrets`.
//! - **Metrics decision:** the original Phase 6 page's three counters
//!   (`captures_count`, `search_count`, `search_duration_ms`) exist as
//!   structured fields on the capture/search events. Cumulative
//!   cross-process counters were researched and rejected: Recall is a
//!   one-shot CLI with no daemon, so a running total would need its own
//!   store — a feature with no consumer until Phase 7's DeployScore
//!   feed, which will consume these per-operation events instead.
//! - Logs go to STDERR only (stdout is reserved for data).

use tracing_subscriber::EnvFilter;

/// Initialize the tracing subscriber.
///
/// Level: `RECALL_LOG` env var if set, otherwise `recall=info`; `--verbose`
/// (the `verbose` flag) raises it to `recall=debug`.
pub fn init(verbose: bool) {
    let filter = if let Ok(level) = std::env::var("RECALL_LOG") {
        EnvFilter::new(level)
    } else if verbose {
        EnvFilter::new("recall=debug")
    } else {
        EnvFilter::new("recall=info")
    };
    // Logs go to STDERR: stdout is reserved for data (search results,
    // `recall export` JSON) and must never be polluted by log lines.
    // (Phase 5 change — required by export-to-stdout.)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
