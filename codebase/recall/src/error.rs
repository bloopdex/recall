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
}

pub type Result<T> = std::result::Result<T, Error>;
