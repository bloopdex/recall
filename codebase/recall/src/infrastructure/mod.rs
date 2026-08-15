//! Infrastructure adapters — SQLite persistence and git metadata.
//!
//! The domain model has no knowledge of these modules; the application
//! layer composes them.

pub mod database;
pub mod embeddings;
pub mod git;
pub mod shell;
