//! Hybrid search tests: deterministic reciprocal-rank fusion of keyword
//! and semantic candidates with synthetic vectors (no model required).

use time::{Duration, OffsetDateTime};

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::{Db, SearchFilter};
use recall::infrastructure::embeddings::EMBED_DIMS;

const MODEL: &str = "all-MiniLM-L6-v2";
const VERSION: &str = "1";

fn unit(fill: f32) -> Vec<f32> {
    let mut v = vec![fill; EMBED_DIMS];
    // Deterministic, non-constant direction.
    for (i, x) in v.iter_mut().enumerate() {
        *x = fill + (i as f32 * 1e-6);
    }
    let l: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter_mut().for_each(|x| *x /= l);
    v
}

fn open() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("recall.db")).unwrap();
    assert!(db.vec_enabled());
    (dir, db)
}

fn capture(db: &mut Db, problem: &str, error: Option<&str>, at: OffsetDateTime) -> i64 {
    db.insert_memory(
        &NewMemory {
            problem: problem.into(),
            solution: "solution".into(),
            error: error.map(String::from),
            ..Default::default()
        },
        at,
    )
    .unwrap()
}

#[test]
fn keyword_only_match_surfaces_via_fts_side() {
    let (_dir, mut db) = open();
    let id = capture(
        &mut db,
        "kafka consumer lag on orders-events",
        None,
        OffsetDateTime::now_utc(),
    );
    // No embedding for this memory: it must still be found by FTS alone.
    let hits = db
        .hybrid_search(
            "kafka consumer lag",
            Some(&unit(1.0)),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, id);
    assert!(hits[0].fts_rank.is_some());
    assert!(hits[0].sem_similarity.is_none());
}

#[test]
fn semantic_only_match_surfaces_via_vector_side() {
    let (_dir, mut db) = open();
    let t = OffsetDateTime::now_utc();
    let id = capture(
        &mut db,
        "Postgres connections were exhausted because transactions weren't being released.",
        None,
        t,
    );
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &unit(1.0))
        .unwrap();

    // The query shares NO keywords with the stored problem — the semantic
    // side must carry it.
    let hits = db
        .hybrid_search(
            "database pool keeps running out of connections",
            Some(&unit(0.99)),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, id);
    assert!(hits[0].fts_rank.is_none());
    assert!(hits[0].sem_similarity.is_some());
}

#[test]
fn hybrid_ranks_dual_match_above_single_match() {
    let (_dir, mut db) = open();
    let t = OffsetDateTime::now_utc();
    // Memory A matches on BOTH engines (exact keyword + strong vector).
    let a = capture(&mut db, "postgres connection pool exhausted", None, t);
    db.insert_embedding(a, MODEL, VERSION, EMBED_DIMS, &unit(1.0))
        .unwrap();
    // Memory B matches semantically only (no keyword overlap).
    let b = capture(
        &mut db,
        "transactions were never released to the database",
        None,
        t,
    );
    db.insert_embedding(b, MODEL, VERSION, EMBED_DIMS, &unit(0.98))
        .unwrap();

    let hits = db
        .hybrid_search(
            "postgres connection pool",
            Some(&unit(0.97)),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].memory.id, a, "dual match must beat semantic-only");
    assert_eq!(hits[1].memory.id, b);
    assert!(hits[0].fts_rank.is_some() && hits[0].sem_similarity.is_some());
    assert!(hits[1].fts_rank.is_none() && hits[1].sem_similarity.is_some());
}

#[test]
fn ranking_is_deterministic_across_runs() {
    let (_dir, mut db) = open();
    let t = OffsetDateTime::now_utc();
    for (i, text) in [
        "postgres connection pool exhausted",
        "connection pool size",
        "sqlite database is locked",
    ]
    .iter()
    .enumerate()
    {
        let id = capture(&mut db, text, None, t + Duration::seconds(i as i64));
        db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &unit(1.0 - i as f32 * 0.01))
            .unwrap();
    }
    let first: Vec<i64> = db
        .hybrid_search(
            "pool exhausted",
            Some(&unit(0.99)),
            &SearchFilter::default(),
            10,
        )
        .unwrap()
        .iter()
        .map(|h| h.memory.id)
        .collect();
    let second: Vec<i64> = db
        .hybrid_search(
            "pool exhausted",
            Some(&unit(0.99)),
            &SearchFilter::default(),
            10,
        )
        .unwrap()
        .iter()
        .map(|h| h.memory.id)
        .collect();
    assert_eq!(first, second, "hybrid search must be deterministic");
}

#[test]
fn ties_break_by_recency() {
    let (_dir, mut db) = open();
    let t = OffsetDateTime::now_utc();
    let older = capture(&mut db, "same problem text", None, t);
    let newer = capture(
        &mut db,
        "same problem text",
        None,
        t + Duration::seconds(60),
    );
    // Identical vectors → identical semantic contribution; identical
    // problem → identical FTS contribution. Newer must win the tie.
    db.insert_embedding(older, MODEL, VERSION, EMBED_DIMS, &unit(1.0))
        .unwrap();
    db.insert_embedding(newer, MODEL, VERSION, EMBED_DIMS, &unit(1.0))
        .unwrap();
    let hits = db
        .hybrid_search(
            "same problem",
            Some(&unit(0.99)),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].memory.id, newer);
    assert_eq!(hits[1].memory.id, older);
}

#[test]
fn missing_query_vector_degrades_to_fts_only() {
    let (_dir, mut db) = open();
    let id = capture(
        &mut db,
        "kafka consumer lag",
        None,
        OffsetDateTime::now_utc(),
    );
    let hits = db
        .hybrid_search("kafka consumer lag", None, &SearchFilter::default(), 10)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, id);
    assert!(hits[0].sem_similarity.is_none());
}

#[test]
fn no_results_is_clean() {
    let (_dir, mut db) = open();
    capture(&mut db, "kafka lag", None, OffsetDateTime::now_utc());
    let hits = db
        .hybrid_search(
            "completely unrelated",
            Some(&unit(0.5)),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert!(hits.is_empty());
}
