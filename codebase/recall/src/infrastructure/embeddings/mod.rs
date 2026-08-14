//! Local embedding generation (ADR-0013).
//!
//! The embedder runs a local all-MiniLM-L6-v2 ONNX model through fastembed,
//! loading model files from a user-defined directory — nothing is ever
//! downloaded at runtime. Model acquisition is the explicit, opt-in
//! `recall embeddings download` command (feature-gated, see download.rs).

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// One-time model download (only compiled with the `download` feature).
#[cfg(feature = "download")]
pub mod download;

/// Canonical embedding model (ADR-0013).
pub const MODEL_ID: &str = "all-MiniLM-L6-v2";
/// Model version for stored-vector compatibility. Bump when the model
/// files change; existing vectors then count as stale and are rebuilt by
/// `recall embeddings build`.
pub const MODEL_VERSION: &str = "1";
/// Embedding dimensionality produced by the model.
pub const EMBED_DIMS: usize = 384;
/// Max token length fed to the model (fastembed default).
const MAX_LENGTH: usize = 512;

const MODEL_DIR_ENV: &str = "RECALL_MODEL_DIR";

/// Directory holding the model files (`model.onnx`, `tokenizer.json`,
/// `config.json`, `special_tokens_map.json`, `tokenizer_config.json`).
pub fn model_dir() -> PathBuf {
    if let Ok(dir) = std::env::var(MODEL_DIR_ENV) {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base)
        .join("recall")
        .join("models")
        .join(MODEL_ID)
}

/// True when the model files are present on disk (no load attempt).
pub fn model_present() -> bool {
    let dir = model_dir();
    [
        "model.onnx",
        "tokenizer.json",
        "config.json",
        "tokenizer_config.json",
    ]
    .iter()
    .all(|f| dir.join(f).is_file())
}

/// Build the embedding input for a memory: the "what happened" side only —
/// problem + error + context. The solution is deliberately excluded: the
/// retrieval question is "have I solved something like this before?", and
/// queries resemble symptom descriptions, not fix wording (ADR-0013).
pub fn embedded_text(problem: &str, error: Option<&str>, context: Option<&str>) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(3);
    parts.push(problem);
    if let Some(e) = error {
        if !e.trim().is_empty() {
            parts.push(e);
        }
    }
    if let Some(c) = context {
        if !c.trim().is_empty() {
            parts.push(c);
        }
    }
    parts.join("\n")
}

/// A loaded local embedder.
pub struct Embedder {
    inner: fastembed::TextEmbedding,
}

impl Embedder {
    /// Load the model from the local directory. Fails with a clear,
    /// actionable message when the model is absent (run
    /// `recall embeddings download`).
    pub fn try_load() -> Result<Self> {
        let dir = model_dir();
        if !model_present() {
            return Err(Error::Embedding(format!(
                "embedding model not installed at {} — run `recall embeddings download` once (network is used only by that command)",
                dir.display()
            )));
        }
        let onnx = std::fs::read(dir.join("model.onnx"))
            .map_err(|e| Error::Embedding(format!("cannot read model.onnx: {e}")))?;
        let tokenizer_files = fastembed::TokenizerFiles {
            tokenizer_file: read_model_file(&dir, "tokenizer.json")?,
            config_file: read_model_file(&dir, "config.json")?,
            special_tokens_map_file: read_model_file(&dir, "special_tokens_map.json")?,
            tokenizer_config_file: read_model_file(&dir, "tokenizer_config.json")?,
        };
        let model_def = fastembed::UserDefinedEmbeddingModel::new(onnx, tokenizer_files)
            .with_pooling(fastembed::Pooling::Mean);
        let mut options = fastembed::InitOptionsUserDefined::default();
        options.max_length = MAX_LENGTH;
        let inner = fastembed::TextEmbedding::try_new_from_user_defined(model_def, options)
            .map_err(|e| Error::Embedding(format!("cannot initialize the embedding model: {e}")))?;
        Ok(Self { inner })
    }

    /// Embed a batch of texts. Empty batch returns empty output.
    pub fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let embeddings = self
            .inner
            .embed(texts.to_vec(), None)
            .map_err(|e| Error::Embedding(format!("embedding failed: {e}")))?;
        if embeddings.iter().any(|e| e.len() != EMBED_DIMS) {
            return Err(Error::Embedding(format!(
                "model produced {} dimensions, expected {EMBED_DIMS}",
                embeddings[0].len()
            )));
        }
        Ok(embeddings)
    }

    /// Embed a single text.
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.embed(&[text])?.pop().unwrap_or_default())
    }
}

fn read_model_file(dir: &Path, name: &str) -> Result<Vec<u8>> {
    std::fs::read(dir.join(name)).map_err(|e| Error::Embedding(format!("cannot read {name}: {e}")))
}
