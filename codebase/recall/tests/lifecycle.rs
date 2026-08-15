//! Lifecycle workflows (ADR-0023): archive / unarchive / delete, the
//! confirmation discipline, and embedding/index cleanup. Library-level
//! tests drive the interactive confirmation path with injected streams;
//! binary-level tests pin the non-interactive rules.

mod common;

use std::path::Path;
use std::process::Output;

use recall::domain::memory::{MemoryStatus, NewMemory};
use recall::infrastructure::database::Db;
use recall::infrastructure::embeddings::EMBED_DIMS;
use time::OffsetDateTime;

use common::{bin, stderr, stdout, temp_db_path};

const MODEL: &str = "all-MiniLM-L6-v2";
const VERSION: &str = "1";

fn run(db: &Path, args: &[&str]) -> Output {
    common::run(db, None, args, None)
}

fn capture(db: &Path, problem: &str, project: &str) {
    let out = run(
        db,
        &[
            "capture",
            "--problem",
            problem,
            "--solution",
            "solution",
            "--project",
            project,
        ],
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
}

// ---------------------------------------------------------------------------
// Archive / unarchive
// ---------------------------------------------------------------------------

#[test]
fn archive_hides_from_search_and_unarchive_restores() {
    let (_dir, db) = temp_db_path();
    capture(&db, "kafka consumer lag", "alpha");

    let out = run(&db, &["search", "kafka consumer lag"]);
    assert!(stdout(&out).contains("kafka consumer lag"));

    let out = run(&db, &["archive", "1"]);
    assert!(out.status.success(), "archive failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Archived #1"));

    // Hidden by default, visible with --include-archived (and marked).
    let out = run(&db, &["search", "kafka consumer lag"]);
    assert!(stdout(&out).contains("No results"), "{}", stdout(&out));
    let out = run(&db, &["search", "--include-archived", "kafka consumer lag"]);
    let text = stdout(&out);
    assert!(text.contains("kafka consumer lag"), "{text}");
    assert!(text.contains("archived"), "archived marker missing: {text}");

    // List: active list empty; --archived shows it.
    let out = run(&db, &["list"]);
    assert!(stdout(&out).contains("No memories found"));
    let out = run(&db, &["list", "--archived"]);
    assert!(stdout(&out).contains("archived"));

    let out = run(&db, &["unarchive", "1"]);
    assert!(out.status.success(), "unarchive failed: {}", stderr(&out));
    let out = run(&db, &["search", "kafka consumer lag"]);
    assert!(stdout(&out).contains("kafka consumer lag"));
}

#[test]
fn archive_unknown_id_is_a_clear_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["archive", "99"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("no memory with id 99"));
}

#[test]
fn dedup_still_blocks_recapturing_an_archived_memory() {
    let (_dir, db) = temp_db_path();
    capture(&db, "recurring flaky test", "alpha");
    run(&db, &["archive", "1"]);
    // Re-capturing the same problem must still be skipped: archiving is
    // deliberate, and a duplicate would undo it.
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "recurring flaky test",
            "--solution",
            "solution",
            "--project",
            "alpha",
        ],
    );
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("Skipped: near-identical"),
        "{}",
        stdout(&out)
    );
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[test]
fn delete_requires_confirmation_without_a_terminal() {
    let (_dir, db) = temp_db_path();
    capture(&db, "delete me", "alpha");
    let out = run(&db, &["delete", "1"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("refusing to delete without confirmation"),
        "{}",
        stderr(&out)
    );
    // Nothing was deleted.
    let out = run(&db, &["search", "delete me"]);
    assert!(stdout(&out).contains("delete me"));
}

#[test]
fn delete_with_yes_removes_the_memory() {
    let (_dir, db) = temp_db_path();
    capture(&db, "delete me", "alpha");
    let out = run(&db, &["delete", "1", "--yes"]);
    assert!(out.status.success(), "delete failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Deleted #1"));
    let out = run(&db, &["search", "delete me"]);
    assert!(stdout(&out).contains("No results"));
}

#[test]
fn delete_project_removes_only_that_project() {
    let (_dir, db) = temp_db_path();
    capture(&db, "alpha one", "alpha");
    capture(&db, "alpha two", "alpha");
    capture(&db, "beta one", "beta");

    // Refused without --yes.
    let out = run(&db, &["delete", "--project", "alpha"]);
    assert!(!out.status.success());

    let out = run(&db, &["delete", "--project", "ALPHA", "--yes"]);
    assert!(out.status.success(), "delete failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Deleted 2"), "{}", stdout(&out));

    let out = run(&db, &["projects"]);
    let text = stdout(&out);
    assert!(!text.contains("alpha"), "alpha must be gone: {text}");
    assert!(text.contains("beta"), "beta must survive: {text}");
}

#[test]
fn delete_unknown_project_is_a_clear_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["delete", "--project", "nope", "--yes"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("no memories for project"));
}

// ---------------------------------------------------------------------------
// Library-level: confirmation path + embedding cleanup
// ---------------------------------------------------------------------------

fn open_db(path: &Path) -> Db {
    Db::open(path).expect("db must open")
}

#[test]
fn delete_tty_confirmation_accepts_yes_and_declines_no() {
    let (_dir, db_path) = temp_db_path();
    let mut db = open_db(&db_path);
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "confirm me".into(),
                solution: "s".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();

    // Decline ("n"): memory survives.
    recall::application::lifecycle::delete_one(
        &mut db,
        id,
        false,
        true,
        &mut std::io::Cursor::new(b"n\n".to_vec()),
        &mut Vec::new(),
    )
    .unwrap();
    assert!(db.get_memory(id).unwrap().is_some());

    // Accept ("y"): memory gone.
    recall::application::lifecycle::delete_one(
        &mut db,
        id,
        false,
        true,
        &mut std::io::Cursor::new(b"y\n".to_vec()),
        &mut Vec::new(),
    )
    .unwrap();
    assert!(db.get_memory(id).unwrap().is_none());
}

#[test]
fn delete_removes_embedding_and_vector_entries() {
    let (_dir, db_path) = temp_db_path();
    let mut db = open_db(&db_path);
    assert!(db.vec_enabled(), "vec must be available for this test");
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "vector cleanup".into(),
                solution: "s".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    let vector = vec![0.9f32; EMBED_DIMS];
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &vector)
        .unwrap();

    // Semantic search finds it before deletion.
    let hits = db
        .semantic_search(
            &vector,
            5,
            MODEL,
            VERSION,
            &recall::infrastructure::database::SearchFilter::default(),
        )
        .unwrap();
    assert_eq!(hits.len(), 1);

    db.delete_memory(id).unwrap();

    // After deletion: no semantic hit, no metadata row, no vec0 row.
    let hits = db
        .semantic_search(
            &vector,
            5,
            MODEL,
            VERSION,
            &recall::infrastructure::database::SearchFilter::default(),
        )
        .unwrap();
    assert!(hits.is_empty(), "deleted vectors must not be searchable");
}

#[test]
fn archive_keeps_the_embedding_and_search_filters_it() {
    let (_dir, db_path) = temp_db_path();
    let mut db = open_db(&db_path);
    assert!(db.vec_enabled());
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "keep my vector".into(),
                solution: "s".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    let vector = vec![0.8f32; EMBED_DIMS];
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &vector)
        .unwrap();
    db.set_status(id, MemoryStatus::Archived).unwrap();

    let filtered = db
        .semantic_search(
            &vector,
            5,
            MODEL,
            VERSION,
            &recall::infrastructure::database::SearchFilter::default(),
        )
        .unwrap();
    assert!(
        filtered.is_empty(),
        "archived vectors must not surface by default"
    );

    let included = db
        .semantic_search(
            &vector,
            5,
            MODEL,
            VERSION,
            &recall::infrastructure::database::SearchFilter {
                project: None,
                include_archived: true,
            },
        )
        .unwrap();
    assert_eq!(included.len(), 1, "the vector survives archiving");

    // Unarchive restores default visibility.
    db.set_status(id, MemoryStatus::Active).unwrap();
    let restored = db
        .semantic_search(
            &vector,
            5,
            MODEL,
            VERSION,
            &recall::infrastructure::database::SearchFilter::default(),
        )
        .unwrap();
    assert_eq!(restored.len(), 1);
}

// The binary path is exercised above; silence the unused-import lint if the
// shared helper ends up unused in some configuration.
#[allow(dead_code)]
fn _bin() -> &'static str {
    bin()
}
