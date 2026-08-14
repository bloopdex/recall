//! Temporary probe: compare Pooling::Mean vs Pooling::None on the
//! sentence-transformers ONNX export (its output is already pooled).

#[test]
fn pooling_comparison() {
    let base = std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA set on Windows");
    let model_dir = std::path::PathBuf::from(base).join("recall/models/all-MiniLM-L6-v2");
    if !model_dir.join("model.onnx").exists() {
        eprintln!("SKIP: model not present");
        return;
    }

    let onnx = std::fs::read(model_dir.join("model.onnx")).unwrap();
    let tokenizer_files = fastembed::TokenizerFiles {
        tokenizer_file: std::fs::read(model_dir.join("tokenizer.json")).unwrap(),
        config_file: std::fs::read(model_dir.join("config.json")).unwrap(),
        special_tokens_map_file: std::fs::read(model_dir.join("special_tokens_map.json")).unwrap(),
        tokenizer_config_file: std::fs::read(model_dir.join("tokenizer_config.json")).unwrap(),
    };

    let docs = vec![
        "Postgres connections were exhausted because transactions weren't being released.",
        "database pool keeps running out of connections",
        "npm install fails with a checksum mismatch",
    ];
    let cos = |a: &[f32], b: &[f32]| {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let la: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let lb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (la * lb)
    };

    for (name, pooling) in [("mean", Some(fastembed::Pooling::Mean)), ("none", None)] {
        let mut model_def =
            fastembed::UserDefinedEmbeddingModel::new(onnx.clone(), tokenizer_files.clone());
        if let Some(p) = pooling {
            model_def = model_def.with_pooling(p);
        }
        let model = fastembed::TextEmbedding::try_new_from_user_defined(
            model_def,
            fastembed::InitOptionsUserDefined::default(),
        )
        .expect("load");
        let embeddings = model.embed(docs.clone(), None).expect("embed");
        eprintln!(
            "pooling={name}: dims={}, sim(pool,pq)={:.3}, sim(pool,npm)={:.3}",
            embeddings[0].len(),
            cos(&embeddings[0], &embeddings[1]),
            cos(&embeddings[0], &embeddings[2]),
        );
    }
}
