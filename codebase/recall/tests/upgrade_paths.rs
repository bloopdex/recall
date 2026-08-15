//! Release upgrade paths (ADR-0031).
//!
//! The compatibility contract for users upgrading from earlier releases:
//! - database schema upgrades are covered by the migration suite
//!   (v1→v3, v2→v3, failure atomicity, backup restore — see
//!   src/infrastructure/database/migrations.rs);
//! - export files from any released version remain importable (format
//!   versioning, ADR-0024) — pinned here with a committed
//!   pre-release export fixture;
//! - archived memories and their status survive upgrades (migration
//!   tests + the fixture import below).

mod common;

use std::path::{Path, PathBuf};
use std::process::Output;

use common::{stderr, stdout};

fn repo_root() -> PathBuf {
    // Tests run with the crate root as the working directory; the repo
    // root is two levels up (codebase/recall → repo).
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is two levels under the repo root")
        .to_path_buf()
}

fn run(db: &Path, args: &[&str]) -> Output {
    common::run(db, None, args, None)
}

#[test]
fn a_pre_release_export_imports_losslessly_into_a_fresh_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");
    let fixture = repo_root().join("fixtures/upgrade/pre-release-export.json");
    assert!(fixture.exists(), "the upgrade fixture must be committed");

    let out = run(&db, &["import", fixture.to_str().unwrap()]);
    assert!(out.status.success(), "import failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Imported 3"), "{}", stdout(&out));

    // Every field arrives, including the archived status and the
    // project-less memory.
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    let (total, archived, no_project): (i64, i64, i64) = db_handle.with_connection(|c| {
        (
            c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
                .unwrap(),
            c.query_row(
                "SELECT count(*) FROM memories WHERE status = 'archived'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            c.query_row(
                "SELECT count(*) FROM memories WHERE project IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        )
    });
    assert_eq!(total, 3, "the roundtrip must be lossless");
    assert_eq!(archived, 1, "archived status must survive");
    assert_eq!(no_project, 1, "project-less memories must survive");

    // The imported data is fully usable: search finds it, check passes.
    let out = run(&db, &["search", "--include-archived", "pool exhaustion"]);
    assert!(stdout(&out).contains("legacy-service"), "{}", stdout(&out));
    let out = run(&db, &["check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn upgrading_a_v1_database_twice_in_a_row_is_idempotent() {
    // The "user upgrades from an old install, then upgrades again later"
    // path: the second open performs no migration work and loses nothing.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recall.db");

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!(
        "../src/infrastructure/database/sql/0001_init.sql"
    ))
    .unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         );
         INSERT INTO schema_migrations (version, name) VALUES (1, 'init');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO memories (problem, solution, captured_at)
         VALUES ('old era memory', 'old fix', '2026-08-10T10:00:00.000Z')",
        [],
    )
    .unwrap();
    drop(conn);

    let db = recall::infrastructure::database::Db::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), 3);
    drop(db);

    // Second open: nothing to migrate, data intact, no backup churn.
    let db = recall::infrastructure::database::Db::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), 3);
    let memories = db.list_memories(10).unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].problem, "old era memory");
}

#[test]
fn export_format_version_one_remains_the_read_contract() {
    // The export format version is the compatibility line: imports reject
    // future format versions (ADR-0024). This pins that the fixture —
    // and every earlier export — stays readable at format_version 1.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");
    let fixture = repo_root().join("fixtures/upgrade/pre-release-export.json");
    let raw = std::fs::read_to_string(&fixture).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["format_version"], 1);
    assert_eq!(
        json["recall_schema_version"], 3,
        "the fixture represents the pre-release schema"
    );

    let out = run(&db, &["import", fixture.to_str().unwrap()]);
    assert!(out.status.success(), "format v1 must stay importable");
}
