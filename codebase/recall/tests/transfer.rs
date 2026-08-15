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

#[allow(dead_code)]
fn _bin() -> &'static str {
    bin()
}
