//! The search workflow: FTS5 keyword search with ranked, human-readable
//! results. No semantic ranking yet (Phase 3).

use std::time::Instant;

use tracing::instrument;

use crate::domain::memory::Memory;
use crate::infrastructure::database::Db;
use crate::Result;

pub const DEFAULT_LIMIT: usize = 20;

#[instrument(skip(db))]
pub fn run(db: &Db, query: &str, limit: usize) -> Result<()> {
    let started = Instant::now();
    let hits = db.search(query, limit)?;
    let duration_ms = started.elapsed().as_millis();

    tracing::info!(
        event = "search.run",
        query,
        results = hits.len(),
        search_duration_ms = duration_ms,
    );

    if hits.is_empty() {
        println!("No results for \"{query}\".");
        return Ok(());
    }

    for (i, hit) in hits.iter().enumerate() {
        print_hit(i + 1, hit);
    }
    Ok(())
}

/// Print one result: rank, problem, solution, project, git commit and
/// capture time — enough to judge usefulness at a glance.
fn print_hit(number: usize, hit: &crate::infrastructure::database::SearchHit) {
    let m: &Memory = &hit.memory;
    println!(
        "#{number}  rank {:.2}  captured {}  id {}",
        hit.rank,
        m.captured_at_local(),
        m.id
    );
    if let Some(project) = &m.project {
        println!("    project: {project}");
    }
    if let Some(commit) = &m.git_commit {
        println!("    commit:  {commit}");
    }
    if let Some(error) = first_line(&m.error) {
        println!("    error:   {error}");
    }
    println!("    problem:  {}", first_line_str(&m.problem));
    println!("    solution: {}", first_line_str(&m.solution));
    println!();
}

fn first_line(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(|t| t.lines().next().unwrap_or(t).to_string())
}

fn first_line_str(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_string()
}
