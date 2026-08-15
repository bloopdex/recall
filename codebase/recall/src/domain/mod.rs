//! Domain model — the canonical entry model (see docs/design/README.md),
//! with the optional-field extensions added later in development.
//!
//! Mapping of the canonical nine fields (see docs/design/README.md):
//! Error → `error`, Context → `context`, Commands/Relevant files →
//! `investigation`, Git commit → `git_commit`, Solution → `solution`,
//! Project → `project`, Timestamp → `captured_at`, Optional explanation →
//! `explanation`. New optional fields: `root_cause`, `verification`,
//! `environment`; new auto-captured fields: `repo_path`, `git_branch`,
//! `git_changed_files`, `cwd`. Required: `problem`, `solution`.

pub mod export;
pub mod memory;
pub mod sanitize;
