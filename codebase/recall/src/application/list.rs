//! List recent memories, newest first. Active memories by default;
//! `--archived` flips to the archived set (ADR-0023), and `--project`
//! scopes to one project label (ADR-0022).

use tracing::instrument;

use crate::domain::memory::MemoryStatus;
use crate::infrastructure::database::{Db, SearchFilter};
use crate::Result;

pub const DEFAULT_LIMIT: usize = 20;

#[instrument(skip(db))]
pub fn run(db: &Db, limit: usize, filter: &SearchFilter) -> Result<()> {
    let memories = db.list_memories_filtered(filter, limit)?;
    if memories.is_empty() {
        println!("No memories found (try --archived to see archived ones).");
        return Ok(());
    }
    for m in &memories {
        let project = m.project.as_deref().unwrap_or("-");
        let commit = m.git_commit.as_deref().unwrap_or("-");
        let status = if m.status == MemoryStatus::Archived {
            " [archived]"
        } else {
            ""
        };
        println!(
            "#{}{}  {}  project: {}  commit: {}\n    {}",
            m.id,
            status,
            m.captured_at_local(),
            project,
            commit,
            m.problem.lines().next().unwrap_or(&m.problem),
        );
    }
    Ok(())
}
