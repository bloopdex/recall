//! Vector-store integration tests: sqlite-vec lifecycle, model metadata,
//! versioning, cascade behavior, and failure modes — all with synthetic
//! vectors (no model required).

use time::OffsetDateTime;

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::{to_blob, Db};
use recall::infrastructure::embeddings::EMBED_DIMS;

const MODEL: &str = "all-MiniLM-L6-v2";
const VERSION: &str = "1";

fn vec(v: f32) -> Vec<f32> {
    vec![v; EMBED_DIMS]
}

fn open() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("recall.db")).unwrap();
    assert!(db.vec_enabled(), "sqlite-vec must be available");
    (dir, db)
}

#[test]
fn insert_query_and_metadata_roundtrip() {
    let (_dir, mut db) = open();
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "pool exhausted".into(),
                solution: "fix".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &vec(1.0))
        .unwrap();

    let hits = db.semantic_search(&vec(0.99), 5, MODEL, VERSION).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, id);
    assert!(
        hits[0].1 < 0.01,
        "distance should be tiny, got {}",
        hits[0].1
    );

    let (total, current, missing) = db.embedding_stats(MODEL, VERSION).unwrap();
    assert_eq!((total, current, missing), (1, 1, 0));
    assert!(db.embedding_backlog(MODEL, VERSION).unwrap().is_empty());
}

#[test]
fn stale_model_versions_are_identified() {
    let (_dir, mut db) = open();
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "old model".into(),
                solution: "fix".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    db.insert_embedding(id, MODEL, "0", EMBED_DIMS, &vec(1.0))
        .unwrap();

    let backlog = db.embedding_backlog(MODEL, VERSION).unwrap();
    assert_eq!(backlog, vec![id], "stale version must be in the backlog");
    // Stale vectors never participate in semantic search.
    let hits = db.semantic_search(&vec(1.0), 5, MODEL, VERSION).unwrap();
    assert!(hits.is_empty());

    // Rebuild with the current version replaces the row.
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &vec(1.0))
        .unwrap();
    assert!(db.embedding_backlog(MODEL, VERSION).unwrap().is_empty());
    assert_eq!(
        db.semantic_search(&vec(1.0), 5, MODEL, VERSION)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn delete_removes_both_row_and_index() {
    let (_dir, mut db) = open();
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "x".into(),
                solution: "y".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &vec(1.0))
        .unwrap();
    db.delete_embedding(id).unwrap();
    assert!(db
        .semantic_search(&vec(1.0), 5, MODEL, VERSION)
        .unwrap()
        .is_empty());
    assert_eq!(db.embedding_stats(MODEL, VERSION).unwrap(), (1, 0, 1));
}

#[test]
fn memory_deletion_cascades_to_embedding_and_index() {
    let (_dir, mut db) = open();
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "x".into(),
                solution: "y".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &vec(1.0))
        .unwrap();

    // Deletion exercises the FK cascade + trigger chain (FTS + vec0).
    assert!(db.delete_memory(id).unwrap());

    assert!(db.get_memory(id).unwrap().is_none());
    assert!(db
        .semantic_search(&vec(1.0), 5, MODEL, VERSION)
        .unwrap()
        .is_empty());
}

#[test]
fn dimension_mismatch_is_rejected() {
    let (_dir, mut db) = open();
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "x".into(),
                solution: "y".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    let err = db
        .insert_embedding(id, MODEL, VERSION, 3, &[1.0, 2.0, 3.0])
        .unwrap_err();
    assert!(err.to_string().contains("dimensions"), "unexpected: {err}");
    assert_eq!(db.embedding_stats(MODEL, VERSION).unwrap(), (1, 0, 1));
}

#[test]
fn blob_encoding_is_little_endian_f32() {
    let blob = to_blob(&[1.0, -2.5]);
    assert_eq!(blob.len(), 8);
    let mut chunks = blob.chunks_exact(4);
    assert_eq!(
        f32::from_le_bytes(chunks.next().unwrap().try_into().unwrap()),
        1.0
    );
    assert_eq!(
        f32::from_le_bytes(chunks.next().unwrap().try_into().unwrap()),
        -2.5
    );
}
