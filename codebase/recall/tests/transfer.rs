//! Export / import (ADR-0024): the portable JSON format, secret
//! redaction, duplicate detection, and validation.

mod common;

use std::path::Path;
use std::process::Output;

use common::{bin, stderr, stdout};

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
            "solution text",
            "--project",
            project,
        ],
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
}

fn export(db: &Path, path: &Path, extra: &[&str]) {
    let mut args = vec!["export", "--path", path.to_str().unwrap()];
    args.extend_from_slice(extra);
    let out = run(db, &args);
    assert!(out.status.success(), "export failed: {}", stderr(&out));
}

fn import(db: &Path, path: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["import", path.to_str().unwrap()];
    args.extend_from_slice(extra);
    run(db, &args)
}

#[test]
fn export_import_roundtrip_preserves_fields_and_status() {
    let dir = tempfile::tempdir().unwrap();
    let db1 = dir.path().join("one.db");
    let db2 = dir.path().join("two.db");
    let export_path = dir.path().join("export.json");

    capture(&db1, "postgres pool exhaustion", "checkout");
    capture(&db1, "kafka lag", "billing");
    run(&db1, &["archive", "2"]);

    export(&db1, &export_path, &[]);
    let raw = std::fs::read_to_string(&export_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["format_version"], 1, "format version field required");
    assert_eq!(json["memories"].as_array().unwrap().len(), 2);
    assert!(
        json["memories"][0].get("id").is_none(),
        "the format must not carry internal ids"
    );
    assert!(
        raw.contains("\"status\": \"archived\""),
        "archived status must be exported"
    );

    let out = import(&db2, &export_path, &[]);
    assert!(out.status.success(), "import failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Imported 2"), "{}", stdout(&out));

    // Everything is back: active search finds the active one only.
    let out = run(&db2, &["search", "postgres pool exhaustion"]);
    assert!(stdout(&out).contains("checkout"));
    let out = run(&db2, &["search", "kafka lag"]);
    assert!(stdout(&out).contains("No results"), "{}", stdout(&out));
    let out = run(&db2, &["search", "--include-archived", "kafka lag"]);
    let text = stdout(&out);
    assert!(text.contains("kafka lag"), "{text}");
    assert!(
        text.contains("archived"),
        "status must survive the roundtrip: {text}"
    );
}

#[test]
fn export_redacts_secrets_by_default_and_opt_in_preserves_them() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let redacted = dir.path().join("redacted.json");
    let raw_path = dir.path().join("raw.json");

    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "npm login failed",
            "--solution",
            "used --password hunter2 instead of a token",
            "--project",
            "web",
        ],
    );
    assert!(out.status.success());

    export(&db, &redacted, &[]);
    let redacted_text = std::fs::read_to_string(&redacted).unwrap();
    assert!(
        !redacted_text.contains("hunter2"),
        "default export must redact secrets"
    );
    assert!(redacted_text.contains("<redacted>"));

    export(&db, &raw_path, &["--include-secrets"]);
    let raw_text = std::fs::read_to_string(&raw_path).unwrap();
    assert!(
        raw_text.contains("hunter2"),
        "opt-in export must keep raw text"
    );
}

#[test]
fn import_skips_duplicates_and_force_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let export_path = dir.path().join("export.json");

    capture(&db, "duplicate me", "alpha");
    export(&db, &export_path, &[]);

    let out = import(&db, &export_path, &[]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("skipped 1 duplicate"),
        "{}",
        stdout(&out)
    );

    let out = import(&db, &export_path, &["--force"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Imported 1"), "{}", stdout(&out));
    let out = run(&db, &["search", "--include-archived", "duplicate me"]);
    let text = stdout(&out);
    let hits = text
        .lines()
        .filter(|l| l.contains("problem:  duplicate me"))
        .count();
    assert_eq!(hits, 2, "force must import the near-duplicate: {text}");
}

#[test]
fn import_rejects_wrong_format_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("bad.json");
    std::fs::write(
        &path,
        r#"{"format_version": 99, "exported_at": "x", "recall_schema_version": 1, "memories": []}"#,
    )
    .unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("format_version"), "{}", stderr(&out));
}

#[test]
fn import_rejects_invalid_entries_before_inserting_anything() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("bad.json");
    std::fs::write(
        &path,
        r#"{"format_version": 1, "exported_at": "2026-08-15T00:00:00.000Z", "recall_schema_version": 3,
           "memories": [
             {"problem": "valid one", "solution": "s", "captured_at": "2026-08-14T10:00:00.000Z"},
             {"problem": "", "solution": "s", "captured_at": "2026-08-14T10:00:00.000Z"}
           ]}"#,
    )
    .unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("required field"), "{}", stderr(&out));
    // All-or-nothing: the valid entry must not have been inserted either.
    let out = run(&db, &["search", "valid one"]);
    assert!(stdout(&out).contains("No results"));
}

#[test]
fn import_rejects_malformed_json() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "this is not json").unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("not a valid Recall export"));
}

#[test]
fn import_rejects_a_future_recall_schema_version() {
    // A newer Recall may emit fields this build does not know; importing
    // would silently drop them. Refuse instead.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("future.json");
    std::fs::write(
        &path,
        r#"{"format_version": 1, "exported_at": "2026-08-15T00:00:00.000Z", "recall_schema_version": 99, "memories": []}"#,
    )
    .unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("newer Recall"), "{}", stderr(&out));
}

#[test]
fn import_rejects_a_bad_timestamp_before_inserting_anything() {
    // All-or-nothing holds for timestamp errors too: a valid entry
    // BEFORE the bad one must not be inserted.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("bad-time.json");
    std::fs::write(
        &path,
        r#"{"format_version": 1, "exported_at": "2026-08-15T00:00:00.000Z", "recall_schema_version": 3,
           "memories": [
             {"problem": "valid first", "solution": "s", "captured_at": "2026-08-14T10:00:00.000Z"},
             {"problem": "bad timestamp", "solution": "s", "captured_at": "not-a-timestamp"}
           ]}"#,
    )
    .unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("captured_at"), "{}", stderr(&out));
    let out = run(&db, &["search", "valid first"]);
    assert!(
        stdout(&out).contains("No results"),
        "the valid entry must not be inserted when a later one is bad"
    );
}

#[test]
fn import_rejects_an_unknown_status_before_inserting_anything() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("bad-status.json");
    std::fs::write(
        &path,
        r#"{"format_version": 1, "exported_at": "2026-08-15T00:00:00.000Z", "recall_schema_version": 3,
           "memories": [
             {"problem": "valid first", "solution": "s", "captured_at": "2026-08-14T10:00:00.000Z"},
             {"problem": "weird status", "solution": "s", "status": "frozen",
              "captured_at": "2026-08-14T10:00:00.000Z"}
           ]}"#,
    )
    .unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("status"), "{}", stderr(&out));
    let out = run(&db, &["search", "valid first"]);
    assert!(stdout(&out).contains("No results"));
}

#[test]
fn import_rejects_a_non_utf8_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    let path = dir.path().join("binary.json");
    std::fs::write(&path, [0xFFu8, 0xFE, 0x00, 0x01, 0x02]).unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not a valid Recall export"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn import_rejects_a_truncated_json_file_and_leaves_the_db_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.db");
    capture(&db, "existing memory", "alpha");
    let path = dir.path().join("truncated.json");
    let full = r#"{"format_version": 1, "exported_at": "2026-08-15T00:00:00.000Z", "recall_schema_version": 3,
                  "memories": [{"problem": "extra", "solution": "s", "captured_at": "2026-08-14T10:00:00.000Z"}]}"#;
    // Cut the file mid-JSON.
    std::fs::write(&path, &full[..full.len() / 2]).unwrap();
    let out = import(&db, &path, &[]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not a valid Recall export"),
        "{}",
        stderr(&out)
    );
    // The existing database is untouched: still exactly one memory.
    let conn = recall::infrastructure::database::Db::open(&db).expect("open db");
    let count: i64 = conn
        .with_connection(|c| c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0)))
        .expect("count");
    assert_eq!(count, 1, "a failed import must not change the database");
}

#[test]
fn large_export_import_roundtrip_is_lossless() {
    // 1,000 memories through the full export → import roundtrip into a
    // fresh database: every entry arrives, including archived status.
    // Inserts go through the library (spawning 1,000 CLI processes would
    // make this test needlessly slow); export/import run through the CLI.
    let dir = tempfile::tempdir().unwrap();
    let db1 = dir.path().join("one.db");
    let db2 = dir.path().join("two.db");
    let export_path = dir.path().join("large.json");

    const N: usize = 1000;
    {
        let mut db = recall::infrastructure::database::Db::open(&db1).expect("open db1");
        for i in 0..N {
            let memory = recall::domain::memory::NewMemory {
                problem: format!("bulk problem number {i}"),
                solution: "bulk solution".into(),
                project: Some(format!("project-{}", i % 5)),
                ..Default::default()
            };
            let id = db
                .insert_memory(&memory, time::OffsetDateTime::now_utc())
                .expect("insert");
            if i == 0 {
                db.set_status(id, recall::domain::memory::MemoryStatus::Archived)
                    .expect("archive first");
            }
        }
    }

    export(&db1, &export_path, &[]);
    let out = import(&db2, &export_path, &[]);
    assert!(out.status.success(), "import failed: {}", stderr(&out));
    assert!(
        stdout(&out).contains(&format!("Imported {N}")),
        "{}",
        stdout(&out)
    );

    // Losslessness: counts and the archived status, verified at the
    // library level (search output caps at the default limit).
    let db = recall::infrastructure::database::Db::open(&db2).expect("open db2");
    let (total, archived): (i64, i64) = db.with_connection(|c| {
        (
            c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
                .unwrap(),
            c.query_row(
                "SELECT count(*) FROM memories WHERE status = 'archived'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        )
    });
    assert_eq!(total, N as i64, "the roundtrip must be lossless");
    assert_eq!(archived, 1, "archived status must survive the roundtrip");
}

#[allow(dead_code)]
fn _bin() -> &'static str {
    bin()
}
