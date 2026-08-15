//! Lifecycle workflows (ADR-0023): archive / unarchive / delete.
//!
//! Archive = hide, keep, recoverable. Delete = permanent. Every
//! destructive path is explicit, identifies what will be removed, and —
//! outside a terminal — requires an explicit `--yes`; inside a terminal
//! it asks. Embeddings and both search indexes follow the memory row via
//! FK cascade and triggers (ADR-0014/0015), so no orphaned vectors can
//! survive either operation.

use std::io::{BufRead, Write};

use crate::domain::memory::MemoryStatus;
use crate::infrastructure::database::Db;
use crate::{Error, Result};

pub fn set_status(db: &mut Db, id: i64, status: MemoryStatus) -> Result<()> {
    if db.set_status(id, status)? {
        println!(
            "{}{} #{}",
            crate::ui::ok(),
            match status {
                MemoryStatus::Archived => "Archived",
                MemoryStatus::Active => "Unarchived",
            },
            id
        );
        Ok(())
    } else {
        Err(Error::InvalidInput(format!("no memory with id {id}")))
    }
}

/// `recall delete <id>`: show what will be deleted, confirm (TTY) or
/// require `--yes` (non-TTY), then delete.
pub fn delete_one(
    db: &mut Db,
    id: i64,
    yes: bool,
    stdin_is_tty: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<()> {
    let memory = db
        .get_memory(id)?
        .ok_or_else(|| Error::InvalidInput(format!("no memory with id {id}")))?;
    writeln!(
        prompt_out,
        "{}Will delete: \"{}\" (project: {})",
        crate::ui::warn(),
        first_line(&memory.problem),
        memory.project.as_deref().unwrap_or("none")
    )?;
    if !confirmed(yes, stdin_is_tty, input, prompt_out)? {
        println!("Not deleted.");
        return Ok(());
    }
    db.delete_memory(id)?;
    println!(
        "{}Deleted #{id} (embedding and search index entries removed with it).",
        crate::ui::ok()
    );
    Ok(())
}

/// `recall delete --project <name>`: bulk deletion of one project's
/// memories, with the same confirmation discipline.
pub fn delete_project(
    db: &mut Db,
    project: &str,
    yes: bool,
    stdin_is_tty: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<()> {
    let count = db
        .project_stats()?
        .into_iter()
        .find(|s| {
            s.project
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(project))
        })
        .map(|s| s.count)
        .unwrap_or(0);
    if count == 0 {
        return Err(Error::InvalidInput(format!(
            "no memories for project '{project}'"
        )));
    }
    writeln!(
        prompt_out,
        "{}Will delete {count} memories from project \"{project}\".",
        crate::ui::warn()
    )?;
    if !confirmed(yes, stdin_is_tty, input, prompt_out)? {
        println!("Not deleted.");
        return Ok(());
    }
    let deleted = db.delete_memories_by_project(project)?;
    println!(
        "{}Deleted {deleted} memories from project \"{project}\" (embeddings and index entries removed with them).",
        crate::ui::ok()
    );
    Ok(())
}

/// Confirmation gate: `--yes` wins in non-interactive contexts; otherwise
/// ask `y/N` and accept only an explicit yes.
fn confirmed(
    yes: bool,
    stdin_is_tty: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<bool> {
    if !stdin_is_tty {
        if yes {
            return Ok(true);
        }
        return Err(Error::InvalidInput(
            "refusing to delete without confirmation — pass --yes to confirm".into(),
        ));
    }
    write!(prompt_out, "Delete? [y/N]: ")?;
    prompt_out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}
