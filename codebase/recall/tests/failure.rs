//! Failure-mode tests: corrupt database, missing directories, invalid
//! input, and malformed search queries.

mod common;

use std::path::PathBuf;

use common::{run, stderr, temp_db_path};

#[test]
fn corrupt_database_fails_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("corrupt.db");
    std::fs::write(&db, b"this is definitely not a sqlite database file").unwrap();

    let out = run(&db, None, &["list"], None);
    assert!(!out.status.success(), "corrupt DB must fail");
    let err = stderr(&out);
    assert!(
        err.contains("database"),
        "user-readable DB error expected: {err}"
    );
}

#[test]
fn database_is_created_in_nested_missing_directories() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("deep").join("nested").join("recall.db");

    let out = run(
        &db,
        None,
        &["capture", "--problem", "deep path", "--solution", "created"],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    assert!(db.exists(), "database must be created on demand");
}

#[test]
fn empty_piped_input_is_invalid() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, None, &["capture", "--solution", "fix"], Some(""));
    assert!(!out.status.success());
    assert!(stderr(&out).contains("problem must not be empty"));
}

#[test]
fn missing_database_directory_is_fine_for_search() {
    let dir = tempfile::tempdir().unwrap();
    let db: PathBuf = dir.path().join("new").join("recall.db");
    let out = run(&db, None, &["search", "anything"], None);
    assert!(
        out.status.success(),
        "fresh DB search must succeed: {}",
        stderr(&out)
    );
}
