//! The search workflow: hybrid FTS5 + semantic search with deterministic
//! reciprocal-rank fusion (ADR-0016). Degrades gracefully to FTS-only
//! when the embedding model or the vector store is unavailable.

use std::time::Instant;

use tracing::instrument;

use crate::domain::memory::Memory;
use crate::infrastructure::database::{Db, HybridHit};
use crate::infrastructure::embeddings::Embedder;
use crate::Result;

pub const DEFAULT_LIMIT: usize = 20;

#[instrument(skip(db))]
pub fn run(db: &Db, query: &str, limit: usize, explain: bool) -> Result<()> {
    let started = Instant::now();

    // Semantic layer is best-effort: model missing or embedding failure
    // degrades to keyword-only search (capture reliability philosophy).
    let embedder = Embedder::try_load().ok();
    let query_vector = match &embedder {
        Some(embedder) => match embedder.embed_one(query) {
            Ok(vector) => Some(vector),
            Err(e) => {
                tracing::warn!(event = "embedding.query_failed", error = %e);
                None
            }
        },
        None => None,
    };

    let hits = db.hybrid_search(query, query_vector.as_deref(), limit)?;
    let duration_ms = started.elapsed().as_millis();

    tracing::info!(
        event = "search.run",
        query,
        results = hits.len(),
        semantic = query_vector.is_some(),
        search_duration_ms = duration_ms,
    );

    if hits.is_empty() {
        println!("No results for \"{query}\".");
        return Ok(());
    }

    for (i, hit) in hits.iter().enumerate() {
        print_hit(i + 1, hit, explain);
    }
    Ok(())
}

/// Print one result: rank, problem, solution, project, git commit and
/// capture time — enough to judge usefulness at a glance. `--explain`
/// adds the per-engine ranking signals behind the fused score.
fn print_hit(number: usize, hit: &HybridHit, explain: bool) {
    let m: &Memory = &hit.memory;
    println!(
        "#{number}  fused {:.4}  captured {}  id {}",
        hit.fused_score,
        m.captured_at_local(),
        m.id
    );
    if explain {
        let fts = hit
            .fts_rank
            .map(|r| format!("{r:.4}"))
            .unwrap_or_else(|| "-".to_string());
        let sem = hit
            .sem_similarity
            .map(|s| format!("{s:.3}"))
            .unwrap_or_else(|| "-".to_string());
        println!("    signals: fts_rank={fts}, semantic_sim={sem}");
    }
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
