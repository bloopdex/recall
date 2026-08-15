//! The search workflow: hybrid FTS5 + semantic search with deterministic
//! reciprocal-rank fusion (ADR-0016). Degrades gracefully to FTS-only
//! when the embedding model or the vector store is unavailable.

use std::time::Instant;

use tracing::instrument;

use crate::domain::memory::{Memory, MemoryStatus};
use crate::infrastructure::database::{Db, HybridHit, SearchFilter};
use crate::infrastructure::embeddings::Embedder;
use crate::Result;

pub const DEFAULT_LIMIT: usize = 20;

// `query` is skipped: search terms are user content and must never appear
// in logs (log-data policy, see the observability module doc).
#[instrument(skip(db, query))]
pub fn run(db: &Db, query: &str, limit: usize, explain: bool, filter: &SearchFilter) -> Result<()> {
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

    let hits = db.hybrid_search(query, query_vector.as_deref(), filter, limit)?;
    let duration_ms = started.elapsed().as_millis();

    // No raw query text in the event — log-data policy: content never
    // leaves through logs (a search could itself contain a secret).
    tracing::info!(
        event = "search.run",
        project = ?filter.project,
        include_archived = filter.include_archived,
        results = hits.len(),
        semantic = query_vector.is_some(),
        search_duration_ms = duration_ms,
    );

    let pretty = crate::ui::pretty();

    if hits.is_empty() {
        println!("No results for \"{query}\".");
        if pretty {
            println!(
                "{}capture this problem now and it becomes searchable: recall capture",
                crate::ui::tip()
            );
        }
        return Ok(());
    }

    if pretty {
        println!();
        println!(
            "{}{} result(s) for \"{query}\"",
            crate::ui::search(),
            hits.len()
        );
        println!();
    }
    for (i, hit) in hits.iter().enumerate() {
        print_hit(i + 1, hit, explain, pretty);
    }
    if pretty {
        println!(
            "{}recall edit <id> refines a memory — --explain shows the ranking details.",
            crate::ui::tip()
        );
        println!();
    }
    Ok(())
}

/// Print one result: enough to judge usefulness at a glance (problem,
/// project, capture time, solution, error). Ranking data appears only
/// with `--explain`. On a terminal the same information is laid out
/// with icons; piped output keeps the compact plain form.
fn print_hit(number: usize, hit: &HybridHit, explain: bool, pretty: bool) {
    let m: &Memory = &hit.memory;
    let signals = || {
        let fts = hit
            .fts_rank
            .map(|r| format!("{r:.4}"))
            .unwrap_or_else(|| "-".to_string());
        let sem = hit
            .sem_similarity
            .map(|s| format!("{s:.3}"))
            .unwrap_or_else(|| "-".to_string());
        format!(
            "signals: fused={:.4}, fts_rank={fts}, semantic_sim={sem}",
            hit.fused_score
        )
    };
    if pretty {
        println!("{}. {}", number, first_line_str(&m.problem));
        let mut meta = Vec::new();
        if let Some(project) = &m.project {
            meta.push(format!("{}{project}", crate::ui::folder()));
        }
        meta.push(format!("{}{}", crate::ui::clock(), m.captured_at_local()));
        meta.push(format!("id {}", m.id));
        if let Some(commit) = &m.git_commit {
            meta.push(format!("commit {commit}"));
        }
        if m.status == MemoryStatus::Archived {
            meta.push("archived".to_string());
        }
        println!("   {}", meta.join("  "));
        if explain {
            println!("    {}", signals());
        }
        if let Some(error) = first_line(&m.error) {
            println!("    error:   {error}");
        }
        println!("    solution: {}", first_line_str(&m.solution));
        println!();
    } else {
        println!("#{number}  captured {}  id {}", m.captured_at_local(), m.id);
        if explain {
            println!("    {}", signals());
        }
        if let Some(project) = &m.project {
            println!("    project: {project}");
        }
        if m.status == MemoryStatus::Archived {
            println!("    status:  archived");
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
}

fn first_line(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(|t| t.lines().next().unwrap_or(t).to_string())
}

fn first_line_str(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_string()
}
