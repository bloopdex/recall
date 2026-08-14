//! Domain model — the canonical entry model from Phase 0, with the
//! optional-field extensions from the Phase 1/2 specification.
//!
//! Mapping of the Phase 0 nine fields (see docs/design/README.md):
//! Error → `error`, Context → `context`, Commands/Relevant files →
//! `investigation`, Git commit → `git_commit`, Solution → `solution`,
//! Project → `project`, Timestamp → `captured_at`, Optional explanation →
//! `explanation`. New optional fields: `root_cause`, `verification`,
//! `environment`; new auto-captured fields: `repo_path`, `git_branch`,
//! `git_changed_files`, `cwd`. Required: `problem`, `solution`.

pub mod memory;
