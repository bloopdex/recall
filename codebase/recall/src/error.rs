//! Application error model.
//!
//! Typed at the library boundary, user-readable at the CLI boundary.
//! Messages are actionable and never leak raw SQL or internal stack details;
//! they also never include captured memory content beyond what the user typed
//! into a prompt themselves (see the zero-network / privacy ADRs).

/// Unified error type for Recall.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The file at the configured path is not a readable Recall database
    /// (Phase 6). The message carries the recovery model: restore the
    /// pre-migration backup or re-import from a Recall export. Recall
    /// never modifies the damaged file.
    #[error(
        "database file is corrupt or not a Recall database: {0}. Recovery: restore the \
         <database>.pre-migration-backup snapshot (taken before the last schema upgrade), \
         or re-import from a Recall export (`recall import <file>`). The damaged file \
         was not modified."
    )]
    DbCorrupt(String),

    #[error("database migration failed: {0}")]
    Migration(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("git metadata unavailable: {0}")]
    Git(String),

    #[error("timestamp error: {0}")]
    Time(String),

    #[error("search failed: {0}")]
    Search(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("shell integration error: {0}")]
    Shell(String),

    #[error("export/import error: {0}")]
    Export(String),

    /// `recall check` found consistency problems (the report is printed to
    /// stdout before this error). The exit code is non-zero so scripts can
    /// gate on it.
    #[error("{0}")]
    CheckFailed(String),
}

pub type Result<T> = std::result::Result<T, Error>;
