//! List recent memories, newest first.

use tracing::instrument;

use crate::infrastructure::database::Db;
use crate::Result;

pub const DEFAULT_LIMIT: usize = 20;

#[instrument(skip(db))]
pub fn run(db: &Db, limit: usize) -> Result<()> {
    let memories = db.list_memories(limit)?;
    if memories.is_empty() {
        println!("No memories captured yet.");
        return Ok(());
    }
    for m in &memories {
        let project = m.project.as_deref().unwrap_or("-");
        let commit = m.git_commit.as_deref().unwrap_or("-");
        println!(
            "#{}  {}  project: {}  commit: {}\n    {}",
            m.id,
            m.captured_at_local(),
            project,
            commit,
            m.problem.lines().next().unwrap_or(&m.problem),
        );
    }
    Ok(())
}
