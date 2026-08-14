//! The canonical memory entry model.
//!
//! Capture first, enrich later: only `problem` and `solution` are required;
//! everything else — including all automatically captured metadata — is
//! optional, so capture works outside Git, outside a project, and with no
//! environment available.

use time::{format_description, macros::format_description, OffsetDateTime};

use crate::{Error, Result};

/// A memory being captured (not yet persisted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewMemory {
    pub problem: String,
    pub solution: String,
    pub error: Option<String>,
    pub context: Option<String>,
    pub investigation: Option<String>,
    pub root_cause: Option<String>,
    pub verification: Option<String>,
    pub environment: Option<String>,
    pub explanation: Option<String>,
    pub project: Option<String>,
    pub repo_path: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit: Option<String>,
    pub git_changed_files: Option<String>,
    pub cwd: Option<String>,
}

impl NewMemory {
    /// Validate required fields. Empty optional fields are allowed and are
    /// normalized to `None` by the caller before validation.
    pub fn validate(&self) -> Result<()> {
        if self.problem.trim().is_empty() {
            return Err(Error::InvalidInput("problem must not be empty".into()));
        }
        if self.solution.trim().is_empty() {
            return Err(Error::InvalidInput("solution must not be empty".into()));
        }
        Ok(())
    }

    /// Trim every field and normalize empty optionals to `None`.
    pub fn normalize(mut self) -> Self {
        let opt = |s: Option<String>| -> Option<String> {
            let t = s?.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        };
        self.problem = self.problem.trim().to_string();
        self.solution = self.solution.trim().to_string();
        self.error = opt(self.error);
        self.context = opt(self.context);
        self.investigation = opt(self.investigation);
        self.root_cause = opt(self.root_cause);
        self.verification = opt(self.verification);
        self.environment = opt(self.environment);
        self.explanation = opt(self.explanation);
        self.project = opt(self.project);
        self.repo_path = opt(self.repo_path);
        self.git_branch = opt(self.git_branch);
        self.git_commit = opt(self.git_commit);
        self.git_changed_files = opt(self.git_changed_files);
        self.cwd = opt(self.cwd);
        self
    }
}

/// A persisted memory, as read back from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    pub id: i64,
    pub problem: String,
    pub solution: String,
    pub error: Option<String>,
    pub context: Option<String>,
    pub investigation: Option<String>,
    pub root_cause: Option<String>,
    pub verification: Option<String>,
    pub environment: Option<String>,
    pub explanation: Option<String>,
    pub project: Option<String>,
    pub repo_path: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit: Option<String>,
    pub git_changed_files: Option<String>,
    pub cwd: Option<String>,
    /// Capture time, stored as UTC, rendered in local time on display.
    pub captured_at: OffsetDateTime,
}

impl Memory {
    /// Render `captured_at` in the machine's local time for display.
    pub fn captured_at_local(&self) -> String {
        const FMT: &[format_description::FormatItem<'_>] =
            format_description!("[year]-[month]-[day] [hour]:[minute]");
        let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
        self.captured_at
            .to_offset(offset)
            .format(FMT)
            .unwrap_or_else(|_| "unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_problem_is_rejected() {
        let m = NewMemory {
            problem: "   ".into(),
            solution: "fix".into(),
            ..Default::default()
        };
        assert!(matches!(m.validate(), Err(Error::InvalidInput(_))));
    }

    #[test]
    fn empty_solution_is_rejected() {
        let m = NewMemory {
            problem: "problem".into(),
            solution: "".into(),
            ..Default::default()
        };
        assert!(matches!(m.validate(), Err(Error::InvalidInput(_))));
    }

    #[test]
    fn normalize_trims_and_drops_empty_optionals() {
        let m = NewMemory {
            problem: "  pool exhausted  ".into(),
            solution: "  raise limit  ".into(),
            context: Some("  ".into()),
            explanation: Some("  detail  ".into()),
            ..Default::default()
        }
        .normalize();
        assert_eq!(m.problem, "pool exhausted");
        assert_eq!(m.solution, "raise limit");
        assert_eq!(m.context, None);
        assert_eq!(m.explanation.as_deref(), Some("detail"));
    }
}
