//! `recall check` — read-only consistency diagnostics (ADR-0028).

mod common;

use std::path::Path;
use std::process::Output;

use recall::infrastructure::database::Db;

use common::{stderr, stdout, temp_db_path};

fn run(db: &Path, args: &[&str]) -> Output {
    common::run(db, None, args, None)
}

fn capture(db: &Path, problem: &str) {
    let out = run(
        db,
        &[
            "capture",
            "--problem",
            problem,
            "--solution",
            "solution text",
            "--project",
            "p",
        ],
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
}

#[test]
fn healthy_database_reports_ok_and_exits_zero() {
    let (_dir, db) = temp_db_path();
    capture(&db, "healthy entry one");
    capture(&db, "healthy entry two");
    let out = run(&db, &["check"]);
    assert!(out.status.success(), "check failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("RESULT: OK"), "{text}");
    assert!(text.contains("memories:"), "{text}");
}

#[test]
fn check_is_read_only() {
    let (_dir, db) = temp_db_path();
    capture(&db, "before the check");
    let before = std::fs::read(&db).expect("read db before");
    let out = run(&db, &["check"]);
    assert!(out.status.success());
    let after = std::fs::read(&db).expect("read db after");
    assert_eq!(
        before, after,
        "recall check must not modify the database file"
    );
}

#[test]
fn invalid_lifecycle_status_is_detected() {
    let (_dir, db) = temp_db_path();
    capture(&db, "status victim");
    {
        let conn = Db::open(&db).expect("open for tampering");
        conn.with_connection(|c| {
            c.execute("UPDATE memories SET status = 'bogus'", [])
                .expect("tamper");
        });
    }
    let out = run(&db, &["check"]);
    assert!(!out.status.success(), "check must fail on invalid status");
    let text = stdout(&out);
    assert!(text.contains("status_validity"), "{text}");
    assert!(text.contains("RESULT: 1 consistency problem"), "{text}");
    assert!(
        text.contains("recall import"),
        "recovery hints must print: {text}"
    );
}

#[test]
fn fts_desync_is_detected() {
    let (_dir, db) = temp_db_path();
    capture(&db, "fts desync victim");
    {
        // Delete one row directly from the FTS index (bypassing the
        // canonical table — the state a failed trigger or manual edit
        // would leave behind).
        let conn = Db::open(&db).expect("open for tampering");
        conn.with_connection(|c| {
            c.execute(
                "INSERT INTO memories_fts(memories_fts, rowid, problem, solution)
                 VALUES('delete', 1, 'fts desync victim', 'solution text')",
                [],
            )
            .expect("tamper");
        });
    }
    let out = run(&db, &["check"]);
    assert!(!out.status.success(), "check must fail on FTS desync");
    let text = stdout(&out);
    assert!(text.contains("fts5_integrity_check"), "{text}");
}

#[test]
fn embedding_orphan_is_detected() {
    let (_dir, db) = temp_db_path();
    capture(&db, "orphan victim");
    {
        // A row in `embeddings` whose memory no longer exists: FK is ON,
        // so disable it on this tampering connection (simulating a
        // foreign-tool edit) and insert the orphan.
        let conn = Db::open(&db).expect("open for tampering");
        conn.with_connection(|c| {
            c.execute_batch(
                "PRAGMA foreign_keys = OFF;
                 INSERT INTO embeddings (memory_id, model, model_version, dims, vector)
                 VALUES (999, 'bench', '1', 384, zeroblob(384 * 4));",
            )
            .expect("tamper");
        });
    }
    let out = run(&db, &["check"]);
    assert!(
        !out.status.success(),
        "check must fail on an orphan embedding"
    );
    let text = stdout(&out);
    assert!(text.contains("embedding_orphans"), "{text}");
}

#[test]
fn vec0_desync_is_detected() {
    let (_dir, db) = temp_db_path();
    capture(&db, "vec desync victim");
    {
        let mut conn = Db::open(&db).expect("open for tampering");
        assert!(conn.vec_enabled(), "vec0 must be available for this test");
        // Insert the embedding through the normal path (both tables
        // synced), then remove the vector row directly — bypassing the
        // sync trigger that would have healed it.
        conn.insert_embedding(
            1,
            "bench",
            "1",
            recall::infrastructure::embeddings::EMBED_DIMS,
            &vec![0.5f32; recall::infrastructure::embeddings::EMBED_DIMS],
        )
        .expect("insert embedding");
        conn.with_connection(|c| {
            c.execute("DELETE FROM embeddings_vec WHERE rowid = 1", [])
                .expect("tamper");
        });
    }
    let out = run(&db, &["check"]);
    assert!(!out.status.success(), "check must fail on vec0 desync");
    let text = stdout(&out);
    assert!(text.contains("vec0_row_count"), "{text}");
}

#[test]
fn corrupt_database_fails_the_cli_with_a_recovery_message() {
    // The check command against a non-database file: the open itself
    // fails with the Phase 6 recovery hint.
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("not-a-db.db");
    std::fs::write(&victim, b"definitely not a database").unwrap();
    let out = run(&victim, &["check"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("not a Recall database"), "{err}");
}
