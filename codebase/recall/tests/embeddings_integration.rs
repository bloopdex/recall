//! Real-model integration tests (fastembed + local MiniLM). Skipped
//! gracefully when the model is not installed (CI without the download
//! step), as documented in ADR-0013.

mod common;

use common::{run_with_model, stderr, stdout, temp_db_path};

fn model_present() -> bool {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_default();
    std::path::Path::new(&base)
        .join("recall/models/all-MiniLM-L6-v2/model.onnx")
        .is_file()
}

#[test]
fn capture_embeds_and_semantic_search_finds_paraphrase() {
    if !model_present() {
        eprintln!("SKIP: model not installed");
        return;
    }
    let (_dir, db) = temp_db_path();
    let out = run_with_model(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "Postgres connections were exhausted because transactions weren't being released.",
            "--solution",
            "release the connection in a finally block",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));

    // Paraphrase query: NO keyword overlap with the stored text — only
    // semantic search can find it.
    let out = run_with_model(
        &db,
        None,
        &["search", "database pool keeps running out of connections"],
        None,
    );
    assert!(out.status.success(), "search failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("finally block"),
        "semantic search must find the paraphrase:\n{text}"
    );

    // And --explain exposes the signals (the flag precedes the query:
    // the query argument is trailing-var-arg).
    let out = run_with_model(
        &db,
        None,
        &[
            "search",
            "--explain",
            "database pool keeps running out of connections",
        ],
        None,
    );
    let text = stdout(&out);
    assert!(
        text.contains("semantic_sim"),
        "explain mode must show signals: {text}"
    );
}

#[test]
fn edit_regenerates_embedding() {
    if !model_present() {
        eprintln!("SKIP: model not installed");
        return;
    }
    let (_dir, db) = temp_db_path();
    run_with_model(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "old pool problem",
            "--solution",
            "fix one",
        ],
        None,
    );
    // Edit the embedded field: the vector must be regenerated, not kept stale.
    let out = run_with_model(
        &db,
        None,
        &[
            "edit",
            "1",
            "--problem",
            "database connections leak when transactions are not released",
        ],
        None,
    );
    assert!(out.status.success(), "edit failed: {}", stderr(&out));

    let out = run_with_model(
        &db,
        None,
        &["search", "pool running out of connections"],
        None,
    );
    let text = stdout(&out);
    assert!(
        text.contains("fix one"),
        "edited memory must be semantically findable: {text}"
    );
}

#[test]
fn build_backfills_existing_memories() {
    if !model_present() {
        eprintln!("SKIP: model not installed");
        return;
    }
    let (_dir, db) = temp_db_path();
    // Capture with the model neutralized → memory exists, no embedding.
    let out = common::run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "sqlite database is locked",
            "--solution",
            "busy_timeout",
        ],
        None,
    );
    assert!(out.status.success());

    let status = run_with_model(&db, None, &["embeddings", "status"], None);
    assert!(status.status.success());
    assert!(
        stdout(&status).contains("missing/stale:1"),
        "unexpected: {}",
        stdout(&status)
    );

    let build = run_with_model(&db, None, &["embeddings", "build"], None);
    assert!(build.status.success(), "build failed: {}", stderr(&build));
    assert!(
        stdout(&build).contains("Embedded 1"),
        "unexpected: {}",
        stdout(&build)
    );

    let status = run_with_model(&db, None, &["embeddings", "status"], None);
    assert!(
        stdout(&status).contains("missing/stale:0"),
        "unexpected: {}",
        stdout(&status)
    );
}

#[test]
fn capture_without_model_still_saves_memory() {
    let (_dir, db) = temp_db_path();
    // common::run neutralizes the model dir — capture must succeed and
    // leave the memory fully keyword-searchable.
    let out = common::run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "kafka lag",
            "--solution",
            "tune fetch",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = common::run(&db, None, &["search", "kafka"], None);
    assert!(stdout(&out).contains("tune fetch"));
}

#[test]
fn edit_without_model_invalidates_stale_vector() {
    if !model_present() {
        eprintln!("SKIP: model not installed");
        return;
    }
    let (_dir, db) = temp_db_path();
    // Embed with the model, then edit the embedded field WITHOUT the model
    // (common::run neutralizes it) — the stale vector must be removed so
    // semantic search can never return outdated content.
    run_with_model(
        &db,
        None,
        &["capture", "--problem", "kafka lag", "--solution", "tune"],
        None,
    );
    let out = common::run(
        &db,
        None,
        &["edit", "1", "--problem", "totally different database thing"],
        None,
    );
    assert!(out.status.success(), "edit failed: {}", stderr(&out));

    // Semantic search for the OLD content must not surface it anymore.
    let out = run_with_model(&db, None, &["search", "kafka consumer lag"], None);
    assert!(
        stdout(&out).contains("No results"),
        "stale vector must not surface: {}",
        stdout(&out)
    );
}
