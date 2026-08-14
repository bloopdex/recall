//! Regression test: semantic search at 10k vectors must return distances —
//! joining the metadata table inside the MATCH query lets SQLite reorder
//! the plan and emit NULL `distance` on larger tables, so the two-step
//! query shape in `Db::semantic_search` is pinned here.

use recall::infrastructure::database::Db;
use recall::infrastructure::embeddings::EMBED_DIMS;

fn synthetic(seed: f64) -> Vec<f32> {
    let mut v = vec![0f32; EMBED_DIMS];
    for (i, x) in v.iter_mut().enumerate() {
        *x = (seed + i as f64 * 1e-5) as f32;
    }
    let l: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter_mut().for_each(|x| *x /= l);
    v
}

#[test]
fn semantic_search_on_ten_thousand_vectors() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("recall.db")).unwrap();
    assert!(db.vec_enabled());

    for i in 0..10_000i64 {
        let id = db
            .insert_memory(
                &recall::domain::memory::NewMemory {
                    problem: format!("problem {i}"),
                    solution: format!("solution {i}"),
                    ..Default::default()
                },
                time::OffsetDateTime::now_utc() - time::Duration::minutes(i),
            )
            .unwrap();
        db.insert_embedding(id, "m", "1", EMBED_DIMS, &synthetic(i as f64))
            .unwrap();
    }

    let hits = db.semantic_search(&synthetic(42.0), 50, "m", "1").unwrap();
    assert_eq!(hits.len(), 50, "k=50 must return 50 rows");
    for (_, distance) in &hits {
        assert!(distance.is_finite(), "distance must never be NULL/NaN");
        // Cosine distance ∈ [0, 2]; allow float epsilon below zero.
        assert!(
            *distance >= -1e-3 && *distance <= 2.0,
            "cosine distance range, got {distance}"
        );
    }

    // Hybrid over the same store also works and stays deterministic.
    let a: Vec<i64> = db
        .hybrid_search("problem 5000", Some(&synthetic(1.0)), 10)
        .unwrap()
        .iter()
        .map(|h| h.memory.id)
        .collect();
    let b: Vec<i64> = db
        .hybrid_search("problem 5000", Some(&synthetic(1.0)), 10)
        .unwrap()
        .iter()
        .map(|h| h.memory.id)
        .collect();
    assert_eq!(a, b);
    assert!(!a.is_empty());
}
