//! `recall version` — the four independently versioned surfaces, in one
//! place (ADR-0031). Useful for support/troubleshooting: an upgrade
//! question can be answered by pasting this output.

use crate::domain::export::FORMAT_VERSION;
use crate::infrastructure::embeddings::{MODEL_ID, MODEL_VERSION};
use crate::Result;

/// Print the version report. `schema_version` is `None` when no database
/// exists yet (the command does not create one — informational commands
/// must not have write side effects).
pub fn run(schema_version: Option<i64>) -> Result<()> {
    println!("{}recall {}", crate::ui::brain(), env!("CARGO_PKG_VERSION"));
    match schema_version {
        Some(v) => println!("database schema: v{v}"),
        None => println!("database schema: n/a (no database yet)"),
    }
    println!("export format:  v{FORMAT_VERSION}");
    println!("embedding model: {MODEL_ID} (model version {MODEL_VERSION})");
    Ok(())
}
