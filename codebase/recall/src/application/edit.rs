//! The edit workflow: update user-provided fields of an existing memory.
//! Automatically captured metadata (project, git fields, cwd, timestamps)
//! is not editable here (ADR-0012). FTS5 stays synchronized through the
//! schema's UPDATE trigger.

use tracing::instrument;

use crate::cli::EditArgs;
use crate::domain::memory::MemoryEdits;
use crate::infrastructure::database::Db;
use crate::infrastructure::embeddings::{embedded_text, Embedder, MODEL_ID, MODEL_VERSION};
use crate::{Error, Result};

#[instrument(skip(db))]
pub fn run(db: &mut Db, args: &EditArgs) -> Result<()> {
    let edits = MemoryEdits {
        problem: args.problem.clone(),
        solution: args.solution.clone(),
        error: args.error.clone(),
        context: args.context.clone(),
        investigation: args.investigation.clone(),
        root_cause: args.root_cause.clone(),
        verification: args.verification.clone(),
        environment: args.environment.clone(),
        explanation: args.explanation.clone(),
    };

    if edits.is_empty() {
        return Err(Error::InvalidInput(
            "provide at least one field to edit, e.g. --solution \"<text>\" (empty text clears an optional field)".into(),
        ));
    }
    edits.validate()?;

    // The embedding covers problem + error + context only. Editing any of
    // those makes the stored vector stale; editing the other fields does not.
    let embedded_fields_touched =
        edits.problem.is_some() || edits.error.is_some() || edits.context.is_some();

    let changed = db.update_memory(args.id, &edits)?;
    if !changed {
        return Err(Error::InvalidInput(format!(
            "no memory with id {}",
            args.id
        )));
    }

    if embedded_fields_touched {
        refresh_embedding(db, args.id);
    }

    tracing::info!(event = "edit.success", id = args.id);
    Ok(())
}

/// Regenerate the embedding after an edit of embedded fields. If the model
/// or store is unavailable, delete the old vector instead — a silently
/// stale embedding must never survive (ADR-0015).
fn refresh_embedding(db: &mut Db, id: i64) {
    if !db.vec_enabled() {
        return;
    }
    let Some(memory) = db.get_memory(id).ok().flatten() else {
        return;
    };
    match Embedder::try_load() {
        Ok(embedder) => {
            let text = embedded_text(
                &memory.problem,
                memory.error.as_deref(),
                memory.context.as_deref(),
            );
            match embedder.embed_one(&text) {
                Ok(vector) => {
                    if let Err(e) =
                        db.insert_embedding(id, MODEL_ID, MODEL_VERSION, vector.len(), &vector)
                    {
                        tracing::warn!(event = "embedding.refresh_store_failed", memory_id = id, error = %e);
                    } else {
                        tracing::info!(event = "embedding.refreshed", memory_id = id);
                    }
                }
                Err(e) => {
                    tracing::warn!(event = "embedding.refresh_failed", memory_id = id, error = %e);
                    let _ = db.delete_embedding(id);
                }
            }
        }
        Err(_) => {
            tracing::info!(event = "embedding.invalidated", memory_id = id);
            let _ = db.delete_embedding(id);
        }
    }
}
