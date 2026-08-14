//! One-time model download — the ONLY network-touching code in Recall.
//!
//! Compiled only with the opt-in `download` feature (ADR-0013); default
//! builds have no network code path at all. Fetches the four model files
//! for all-MiniLM-L6-v2 from Hugging Face into the local model directory.

use std::io::Write;
use std::path::Path;

use crate::Result;

const BASE_URL: &str = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main";
const FILES: &[(&str, &str)] = &[
    ("config.json", "config.json"),
    ("tokenizer.json", "tokenizer.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
    ("onnx/model.onnx", "model.onnx"),
];

/// Download the model files into the local model directory.
pub fn download_model() -> Result<()> {
    let dir = crate::infrastructure::embeddings::model_dir();
    std::fs::create_dir_all(&dir)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| crate::Error::Embedding(format!("cannot create HTTP client: {e}")))?;

    for (remote, local) in FILES {
        let target = dir.join(local);
        eprintln!("downloading {remote} ...");
        let response = client
            .get(format!("{BASE_URL}/{remote}"))
            .send()
            .map_err(|e| crate::Error::Embedding(format!("download failed for {remote}: {e}")))?;
        if !response.status().is_success() {
            return Err(crate::Error::Embedding(format!(
                "download failed for {remote}: HTTP {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| crate::Error::Embedding(format!("download failed for {remote}: {e}")))?;
        let mut file = std::fs::File::create(&target)?;
        file.write_all(&bytes)?;
        eprintln!("  wrote {} ({} bytes)", target.display(), bytes.len());
    }
    eprintln!("model ready at {}", dir.display());
    verify_files(&dir)
}

fn verify_files(dir: &Path) -> Result<()> {
    if crate::infrastructure::embeddings::model_present() {
        Ok(())
    } else {
        Err(crate::Error::Embedding(format!(
            "model files incomplete in {} — re-run the command",
            dir.display()
        )))
    }
}
