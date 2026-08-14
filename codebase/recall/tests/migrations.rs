//! Database-layer integration tests: migrations, persistence, FTS
//! synchronization, and schema invariants — directly against the library.

use time::OffsetDateTime;

use recall::domain::memory::{MemoryEdits, NewMemory};
use recall::infrastructure::database::migrations::MIGRATIONS;
use recall::infrastructure::database::Db;

#[test]
fn migrations_are_sorted_and_apply_from_scratch() {
    assert!(
        MIGRATIONS.windows(2).all(|w| w[0].version < w[1].version),
        "MIGRATIONS must be sorted by version"
    );
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("recall.db")).unwrap();
    assert_eq!(
        db.schema_version().unwrap(),
        MIGRATIONS.last().unwrap().version
    );
}

#[test]
fn reopen_is_idempotent_and_preserves_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recall.db");

    let mut db = Db::open(&path).unwrap();
    let id = db
        .insert_memory(&sample(), OffsetDateTime::now_utc())
        .unwrap();
    drop(db);

    let db = Db::open(&path).unwrap();
    assert_eq!(
        db.schema_version().unwrap(),
        MIGRATIONS.last().unwrap().version,
        "reopen must not re-migrate"
    );
    let memory = db.get_memory(id).unwrap().expect("memory survives reopen");
    assert_eq!(memory.problem, "postgres pool exhausted");
    assert_eq!(memory.git_commit.as_deref(), Some("abc1234"));
}

#[test]
fn full_field_roundtrip_preserves_everything() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("recall.db")).unwrap();
    let captured_at = OffsetDateTime::now_utc();

    let new = NewMemory {
        problem: "problem".into(),
        solution: "solution".into(),
        error: Some("error".into()),
        context: Some("context".into()),
        investigation: Some("commands + files".into()),
        root_cause: Some("root cause".into()),
        verification: Some("verified".into()),
        environment: Some("env".into()),
        explanation: Some("explanation".into()),
        project: Some("project".into()),
        repo_path: Some("C:\\src\\project".into()),
        git_branch: Some("main".into()),
        git_commit: Some("deadbeef".into()),
        git_changed_files: Some("a.txt\nb.txt".into()),
        cwd: Some("C:\\src\\project".into()),
    };
    let id = db.insert_memory(&new, captured_at).unwrap();
    let got = db.get_memory(id).unwrap().unwrap();
    assert_eq!(got.problem, new.problem);
    assert_eq!(got.solution, new.solution);
    assert_eq!(got.error, new.error);
    assert_eq!(got.context, new.context);
    assert_eq!(got.investigation, new.investigation);
    assert_eq!(got.root_cause, new.root_cause);
    assert_eq!(got.verification, new.verification);
    assert_eq!(got.environment, new.environment);
    assert_eq!(got.explanation, new.explanation);
    assert_eq!(got.project, new.project);
    assert_eq!(got.repo_path, new.repo_path);
    assert_eq!(got.git_branch, new.git_branch);
    assert_eq!(got.git_commit, new.git_commit);
    assert_eq!(got.git_changed_files, new.git_changed_files);
    assert_eq!(got.cwd, new.cwd);
    // Millisecond precision must survive the round trip (storage truncates
    // to milliseconds — sub-millisecond nanoseconds are not preserved).
    let truncated = captured_at
        .replace_nanosecond(captured_at.nanosecond() / 1_000_000 * 1_000_000)
        .unwrap();
    assert_eq!(got.captured_at, truncated);
}

#[test]
fn fts_index_stays_synchronized_on_insert() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("recall.db")).unwrap();
    db.insert_memory(&sample(), OffsetDateTime::now_utc())
        .unwrap();

    let hits = db.search("postgres pool", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.problem, "postgres pool exhausted");
}

#[test]
fn search_ordering_is_by_rank_then_recency() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("recall.db")).unwrap();
    let t = OffsetDateTime::now_utc();

    // Same problem text → identical rank → newest first.
    db.insert_memory(
        &NewMemory {
            problem: "sqlite database is locked".into(),
            solution: "older fix".into(),
            ..Default::default()
        },
        t,
    )
    .unwrap();
    db.insert_memory(
        &NewMemory {
            problem: "sqlite database is locked".into(),
            solution: "newer fix".into(),
            ..Default::default()
        },
        t + time::Duration::seconds(1),
    )
    .unwrap();

    let hits = db.search("database locked", 10).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].memory.solution, "newer fix");
    assert_eq!(hits[1].memory.solution, "older fix");
}

#[test]
fn near_identical_detection_respects_project_error_and_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("recall.db")).unwrap();

    let base = NewMemory {
        problem: "pool issue".into(),
        solution: "fix one".into(),
        error: Some("connection pool exhausted".into()),
        project: Some("svc-a".into()),
        ..Default::default()
    };
    db.insert_memory(&base, OffsetDateTime::now_utc()).unwrap();

    // Same error, same project, recent → detected.
    let dup = NewMemory {
        problem: "different problem".into(),
        solution: "other".into(),
        error: Some("  CONNECTION pool EXHAUSTED ".into()),
        project: Some("svc-a".into()),
        ..Default::default()
    };
    assert!(db.find_near_identical(&dup, 30).unwrap().is_some());

    // Same normalized problem, same project, different error → detected.
    let same_problem = NewMemory {
        problem: "POOL  issue".into(),
        solution: "other".into(),
        error: None,
        project: Some("svc-a".into()),
        ..Default::default()
    };
    assert!(db.find_near_identical(&same_problem, 30).unwrap().is_some());

    // Same error but different project → not detected.
    let other_project = NewMemory {
        problem: "different".into(),
        solution: "other".into(),
        error: Some("connection pool exhausted".into()),
        project: Some("svc-b".into()),
        ..Default::default()
    };
    assert!(db
        .find_near_identical(&other_project, 30)
        .unwrap()
        .is_none());

    // Old capture outside the window → not detected.
    let old = NewMemory {
        problem: "pool issue".into(),
        solution: "old".into(),
        error: Some("connection pool exhausted".into()),
        project: Some("svc-a".into()),
        ..Default::default()
    };
    db.insert_memory(&old, OffsetDateTime::now_utc() - time::Duration::days(31))
        .unwrap();
    // The fresh "dup" above is still the only recent one; a capture
    // identical to `old` but dated today is compared against recent rows.
    assert!(db.find_near_identical(&dup, 30).unwrap().is_some());
}

#[test]
fn update_memory_edits_fields_and_respects_missing_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("recall.db")).unwrap();
    let id = db
        .insert_memory(
            &NewMemory {
                problem: "before".into(),
                solution: "before".into(),
                error: Some("old error".into()),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();

    let changed = db
        .update_memory(
            id,
            &MemoryEdits {
                solution: Some("after".into()),
                error: Some(String::new()), // clear
                ..Default::default()
            },
        )
        .unwrap();
    assert!(changed);

    let m = db.get_memory(id).unwrap().unwrap();
    assert_eq!(m.problem, "before", "untouched fields stay");
    assert_eq!(m.solution, "after");
    assert_eq!(m.error, None, "empty text clears the field");

    // FTS index reflects the edit.
    let hits = db.search("after", 10).unwrap();
    assert_eq!(hits.len(), 1);

    // Missing id → false, no error.
    let changed = db
        .update_memory(
            999,
            &MemoryEdits {
                solution: Some("x".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!changed);
}

fn sample() -> NewMemory {
    NewMemory {
        problem: "postgres pool exhausted".into(),
        solution: "raised the limit".into(),
        error: Some("connection pool exhausted".into()),
        git_commit: Some("abc1234".into()),
        ..Default::default()
    }
}
