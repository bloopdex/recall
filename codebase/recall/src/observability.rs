//! Observability — lightweight structured logging for a local CLI.
//!
//! Phase 1/2 approach per the BloopLab standard: JSON-able structured events
//! via `tracing`, no external sinks, no telemetry, nothing ever leaves the
//! machine. Named counters (`captures_count`, `search_duration_ms`) are
//! emitted as structured fields on the corresponding events.

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
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
