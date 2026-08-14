//! The edit workflow: update user-provided fields of an existing memory.
//! Automatically captured metadata (project, git fields, cwd, timestamps)
//! is not editable here (ADR-0012). FTS5 stays synchronized through the
//! schema's UPDATE trigger.

use tracing::instrument;

use crate::cli::EditArgs;
use crate::domain::memory::MemoryEdits;
use crate::infrastructure::database::Db;
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

    let changed = db.update_memory(args.id, &edits)?;
    if !changed {
        return Err(Error::InvalidInput(format!(
            "no memory with id {}",
            args.id
        )));
    }

    tracing::info!(event = "edit.success", id = args.id);
    Ok(())
}
