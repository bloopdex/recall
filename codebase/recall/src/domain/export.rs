//! The portable export/import format (ADR-0024).
//!
//! A JSON envelope with named fields and **no internal database ids** — a
//! Recall export is data, not a dump: it can be inspected, diffed, edited,
//! and imported into a different machine or a fresh database. Embeddings
//! are deliberately not part of the format: they are derived data tied to
//! a model/version and are rebuilt locally by `recall embeddings build`
//! after import.
//!
//! Privacy (ADR-0018): export is opt-in, and by default
//! every field passes through the secret sanitizer before serialization;
//! `--include-secrets` exports the raw text explicitly.

use serde::{Deserialize, Serialize};

use crate::domain::memory::MemoryStatus;

/// The export format version. Import rejects anything else.
pub const FORMAT_VERSION: u32 = 1;

/// One memory in portable form. Field names mirror the domain model;
/// nothing here is a database id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportMemory {
    pub problem: String,
    pub solution: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub investigation: Option<String>,
    #[serde(default)]
    pub root_cause: Option<String>,
    #[serde(default)]
    pub verification: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub repo_path: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_commit: Option<String>,
    #[serde(default)]
    pub git_changed_files: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    /// RFC3339 UTC capture timestamp.
    pub captured_at: String,
    /// `active` | `archived` (default active).
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    MemoryStatus::ACTIVE.to_string()
}

/// The top-level export document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportFile {
    /// Must equal [`FORMAT_VERSION`]; import rejects anything else.
    pub format_version: u32,
    /// RFC3339 UTC timestamp of the export.
    pub exported_at: String,
    /// The Recall schema version of the source database, for future
    /// forward-compatibility decisions (informational today).
    pub recall_schema_version: i64,
    pub memories: Vec<ExportMemory>,
}
