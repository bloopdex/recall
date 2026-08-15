//! The embedding maintenance commands: status, build (backfill), download.
//!
//! All fully local except `download`, which is the explicit opt-in model
//! acquisition step (ADR-0013).

use tracing::instrument;

use crate::infrastructure::database::Db;
use crate::infrastructure::embeddings::{embedded_text, Embedder, MODEL_ID, MODEL_VERSION};
use crate::Result;

/// Show model presence and embedding coverage.
#[instrument(skip(db))]
pub fn status(db: &Db) -> Result<()> {
    let dir = crate::infrastructure::embeddings::model_dir();
    let present = crate::infrastructure::embeddings::model_present();
    println!("model:         {MODEL_ID} (version {MODEL_VERSION})");
    println!("model dir:     {}", dir.display());
    println!(
        "model files:   {}",
        if present {
            "present"
        } else {
            "missing — run `recall embeddings download`"
        }
    );
    println!(
        "vector store:  {}",
        if db.vec_enabled() {
            "enabled (sqlite-vec)"
        } else {
            "unavailable (keyword search only)"
        }
    );
    let (total, current, missing) = db.embedding_stats(MODEL_ID, MODEL_VERSION)?;
    println!("memories:      {total}");
    println!("embedded:      {current} (current model)");
    println!("missing/stale:{missing}");
    Ok(())
}

/// Backfill: embed every memory that lacks a current-model embedding.
/// Skips up-to-date memories; failures on one memory do not abort the run.
#[instrument(skip(db))]
pub fn build(db: &mut Db) -> Result<()> {
    let embedder = Embedder::try_load()?;
    let backlog = db.embedding_backlog(MODEL_ID, MODEL_VERSION)?;
    if backlog.is_empty() {
        println!("All memories already have current embeddings.");
        return Ok(());
    }

    println!(
        "Embedding {} memories (model {MODEL_ID} v{MODEL_VERSION})...",
        backlog.len()
    );
    let mut done = 0usize;
    let mut failed = 0usize;
    let mut batch: Vec<(i64, String)> = Vec::new();

    for id in &backlog {
        if let Some(memory) = db.get_memory(*id)? {
            let text = embedded_text(
                &memory.problem,
                memory.error.as_deref(),
                memory.context.as_deref(),
            );
            batch.push((*id, text));
        }
    }

    for chunk in batch.chunks(32) {
        let texts: Vec<&str> = chunk.iter().map(|(_, t)| t.as_str()).collect();
        match embedder.embed(&texts) {
            Ok(vectors) => {
                for ((id, _), vector) in chunk.iter().zip(vectors) {
                    match db.insert_embedding(*id, MODEL_ID, MODEL_VERSION, vector.len(), &vector) {
                        Ok(()) => {
                            done += 1;
                            tracing::info!(event = "embedding.indexed", memory_id = id);
                        }
                        Err(e) => {
                            failed += 1;
                            tracing::warn!(event = "embedding.store_failed", memory_id = id, error = %e);
                        }
                    }
                }
            }
            Err(e) => {
                failed += chunk.len();
                tracing::warn!(event = "embedding.batch_failed", count = chunk.len(), error = %e);
            }
        }
    }

    println!(
        "{}Embedded {done} memories, {failed} failures.",
        if failed == 0 {
            crate::ui::ok()
        } else {
            crate::ui::warn()
        }
    );
    if failed > 0 {
        println!("Re-run `recall embeddings build` to retry the failures.");
    }
    Ok(())
}

/// One-time model download (opt-in network step).
#[instrument]
pub fn download() -> Result<()> {
    #[cfg(feature = "download")]
    {
        crate::infrastructure::embeddings::download::download_model()
    }
    #[cfg(not(feature = "download"))]
    {
        Err(crate::Error::Embedding(
            "this build was compiled without the `download` feature — rebuild with `--features download` to fetch the model".into(),
        ))
    }
}
