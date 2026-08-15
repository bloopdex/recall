//! CI failure snapshot for `recall capture --from-ci` (ADR-0030).
//!
//! GitHub Actions is the first (and only) supported CI target. The step
//! pattern is an explicit opt-in in the user's workflow:
//!
//! ```yaml
//! - name: Capture failure in Recall
//!   if: failure()
//!   run: |
//!     tail -n 100 build.log | recall capture --from-ci \
//!       --step "build" --solution "the remediation you know"
//! ```
//!
//! Privacy model (same as the shell snapshot, ADR-0017/0018): ONLY the
//! whitelist below is read — never `std::env::vars()`. The piped log
//! passes through the sanitizer and the confirmation gate; in
//! non-interactive CI, redacted content fails closed (nothing stored).
//!
//! The problem text is built deterministically from workflow/job/event
//! (NOT the run id), so repeated failures of the same job hit the
//! existing deduplication (ADR-0011) instead of piling up.

use crate::{Error, Result};

/// Whitelisted GitHub Actions environment variables (ADR-0030).
pub const CI_WORKFLOW: &str = "GITHUB_WORKFLOW";
pub const CI_JOB: &str = "GITHUB_JOB";
pub const CI_EVENT: &str = "GITHUB_EVENT_NAME";
pub const CI_REPOSITORY: &str = "GITHUB_REPOSITORY";
pub const CI_SHA: &str = "GITHUB_SHA";
pub const CI_REF: &str = "GITHUB_REF_NAME";
pub const CI_RUN_ID: &str = "GITHUB_RUN_ID";
pub const CI_RUN_ATTEMPT: &str = "GITHUB_RUN_ATTEMPT";
pub const CI_SERVER_URL: &str = "GITHUB_SERVER_URL";
pub const RUNNER_OS: &str = "RUNNER_OS";

/// The snapshot `--from-ci` is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CiSnapshot {
    /// `GITHUB_WORKFLOW` — the workflow name; required.
    pub workflow: String,
    /// `GITHUB_JOB` — the job id; may be empty outside GitHub.
    pub job: String,
    /// `GITHUB_EVENT_NAME` (push, pull_request, …); may be empty.
    pub event: String,
    /// `GITHUB_REPOSITORY` (owner/repo); `None` outside GitHub.
    pub repository: Option<String>,
    /// `GITHUB_SHA` — the commit being built.
    pub sha: Option<String>,
    /// `GITHUB_REF_NAME` — branch or tag name.
    pub ref_name: Option<String>,
    /// `GITHUB_RUN_ID` — unique per run; kept OUT of the problem text so
    /// repeated failures deduplicate.
    pub run_id: Option<String>,
    /// `GITHUB_RUN_ATTEMPT` — re-run counter.
    pub run_attempt: Option<String>,
    /// `GITHUB_SERVER_URL` (https://github.com).
    pub server_url: Option<String>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Read the whitelisted CI snapshot. Fails when `GITHUB_WORKFLOW` is not
/// set — `--from-ci` only makes sense inside a GitHub Actions run.
pub fn read_snapshot() -> Result<CiSnapshot> {
    let workflow = env(CI_WORKFLOW).ok_or_else(|| {
        Error::InvalidInput(
            "no CI failure context found (GITHUB_WORKFLOW is not set) — \
             `capture --from-ci` runs inside a GitHub Actions step, e.g. \
             `if: failure()`: `tail -n 100 build.log | recall capture \
             --from-ci --step build --solution \"...\"`"
                .into(),
        )
    })?;
    Ok(CiSnapshot {
        workflow,
        job: env(CI_JOB).unwrap_or_default(),
        event: env(CI_EVENT).unwrap_or_default(),
        repository: env(CI_REPOSITORY),
        sha: env(CI_SHA),
        ref_name: env(CI_REF),
        run_id: env(CI_RUN_ID),
        run_attempt: env(CI_RUN_ATTEMPT),
        server_url: env(CI_SERVER_URL),
    })
}

/// The repository name part of `GITHUB_REPOSITORY` (owner/repo → repo),
/// used as the project label — consistent with the name-based project
/// identity (ADR-0021).
pub fn repository_name(repository: &str) -> Option<&str> {
    repository.rsplit('/').next().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_name_takes_the_last_segment() {
        assert_eq!(repository_name("owner/repo"), Some("repo"));
        assert_eq!(repository_name("repo"), Some("repo"));
        assert_eq!(repository_name("a/b/c"), Some("c"));
        assert_eq!(repository_name(""), None);
        assert_eq!(repository_name("/"), None);
    }
}
