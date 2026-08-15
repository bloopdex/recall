//! Crash / recovery behavior (Phase 6).
//!
//! The recovery model under test (ADR-0027): every write is a single
//! SQLite transaction, WAL journaling makes committed transactions
//! crash-safe, and a database that cannot be recovered by SQLite fails
//! loudly — never silently — with a message pointing at the recovery
//! options (the pre-migration backup, or re-importing a Recall export).
//!
//! Tests:
//! - a process killed mid-capture (repeatedly, at different points in the
//!   write) never leaves a partial memory or a corrupted index
//! - truncated database files fail cleanly
//! - a zeroed file is reported as "not a Recall database" with the
//!   recovery hint
//! - byte-level page corruption is always detected by `PRAGMA
//!   integrity_check` (either the open fails, or the check reports it)

mod common;

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use recall::infrastructure::database::Db;

use common::{bin, stderr, stdout, temp_db_path};

fn run(db: &Path, args: &[&str]) -> Output {
    common::run(db, None, args, None)
}

/// Open a database and assert it is internally healthy: integrity_check
/// reports "ok", and every memory row is complete.
fn assert_healthy(db_path: &Path) {
    let db = Db::open(db_path).expect("database must open after the crash");
    db.with_connection(|c| {
        let report: String = c
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .expect("integrity check");
        assert_eq!(report, "ok", "integrity_check must pass: {report}");
    });
    db.with_connection(|c| {
        let mut stmt = c
            .prepare("SELECT id, problem, solution FROM memories")
            .expect("select");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .expect("query");
        for row in rows {
            let (id, problem, solution) = row.expect("row");
            assert!(id > 0, "invalid row id");
            assert!(!problem.is_empty(), "partial row: empty problem");
            assert!(!solution.is_empty(), "partial row: empty solution");
        }
    });
}

#[test]
fn killing_a_process_mid_capture_never_leaves_partial_data() {
    // Kill `recall capture` at different points (including mid-write of a
    // large row). SQLite's transactional writes + WAL must leave the
    // database exactly as it was before each attempt: either the whole
    // memory is there, or nothing of it is.
    let (_dir, db) = temp_db_path();
    let dir = _dir.path();

    // A ~15 KB solution makes the insert span multiple pages, widening
    // the window in which the kill can land mid-write.
    let big_solution = "x".repeat(15_000);
    for (round, delay_ms) in [5u64, 10, 30, 80, 150, 300].iter().enumerate() {
        let mut child = Command::new(bin())
            .arg("--db")
            .arg(&db)
            .args(["capture", "--problem"])
            .arg(format!("crash round {round}"))
            .arg("--solution")
            .arg(&big_solution)
            .arg("--project")
            .arg("crash")
            .env("RECALL_MODEL_DIR", dir.join("no-model-dir"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("recall must spawn");
        std::thread::sleep(Duration::from_millis(*delay_ms));
        let _ = child.kill();
        let _ = child.wait();
        assert_healthy(&db);
    }

    // Whatever survived, the store stays consistent and usable: a fresh
    // capture still works.
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "post crash capture",
            "--solution",
            "still works",
            "--project",
            "crash",
        ],
    );
    assert!(
        out.status.success(),
        "capture after crashes: {}",
        stderr(&out)
    );
    assert_healthy(&db);
}

#[test]
fn truncated_database_files_fail_cleanly() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "before truncation",
            "--solution",
            "s",
            "--project",
            "p",
        ],
    );
    assert!(out.status.success());
    drop(out);

    // Close connections (they are per-process; the CLI process has
    // exited), checkpoint, and truncate the file at several points.
    let bytes = std::fs::read(&db).expect("read db");
    assert!(bytes.len() > 1024, "db must have real content");
    for fraction in [0.1f64, 0.5, 0.9] {
        let cut = ((bytes.len() as f64 * fraction) as usize).max(1);
        let victim = db.with_extension(format!("trunc-{fraction}.db"));
        std::fs::write(&victim, &bytes[..cut]).expect("write truncated db");

        let out = run(&victim, &["search", "before truncation"]);
        assert!(
            !out.status.success(),
            "a truncated database must not open silently (fraction {fraction})"
        );
        let err = stderr(&out);
        assert!(
            err.contains("database") || err.contains("Database"),
            "truncated database must fail with a database error: {err}"
        );
    }
}

#[test]
fn zeroed_database_file_is_reported_with_the_recovery_hint() {
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("zeroed.db");
    std::fs::write(&victim, vec![0u8; 4096]).expect("write zeroed file");

    let out = run(&victim, &["search", "anything"]);
    assert!(!out.status.success(), "a zeroed file must not open");
    let err = stderr(&out);
    assert!(
        err.contains("not a Recall database"),
        "the error must name the cause: {err}"
    );
    assert!(
        err.contains("pre-migration-backup") && err.contains("recall import"),
        "the recovery model must be in the message: {err}"
    );
}

#[test]
fn structural_page_corruption_is_detected() {
    // Damage b-tree STRUCTURE: flip bytes inside page headers (at 4096-byte
    // page boundaries). Either the open fails, or `PRAGMA integrity_check`
    // reports the damage. Structural corruption going undetected would be
    // a silent-data-loss bug.
    let (_dir, db) = temp_db_path();
    for i in 0..50 {
        let out = run(
            &db,
            &[
                "capture",
                "--problem",
                &format!("corruption seed {i}"),
                "--solution",
                "some solution text to fill pages",
                "--project",
                "p",
            ],
        );
        assert!(
            out.status.success(),
            "seed capture failed: {}",
            stderr(&out)
        );
    }

    let mut bytes = std::fs::read(&db).expect("read db");
    let len = bytes.len();
    let mut flipped = 0;
    // Page headers live at each 4096-byte boundary; bytes 0-100 of each
    // page hold the page type and cell pointers.
    for k in 1..30 {
        let pos = k * 4096 + 24;
        if pos + 100 < len {
            bytes[pos] ^= 0xFF;
            bytes[pos + 48] ^= 0xFF;
            flipped += 2;
        }
    }
    assert!(flipped >= 10, "the corruption must actually flip bytes");

    let victim = db.with_extension("corrupt.db");
    std::fs::write(&victim, &bytes).expect("write corrupted db");

    match Db::open(&victim) {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("database"),
                "open failure must be a database error: {msg}"
            );
        }
        Ok(db) => {
            let report: String = db.with_connection(|c| {
                c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                    .expect("integrity check")
            });
            assert_ne!(
                report, "ok",
                "structural page corruption must be detected by integrity_check"
            );
        }
    }
}

#[test]
fn payload_content_flips_are_a_documented_detection_limitation() {
    // Honest boundary of the guarantee: SQLite pages carry no content
    // checksums, so `PRAGMA integrity_check` verifies STRUCTURE (b-trees,
    // page types, free-list) but not cell payload bytes. A bit flip inside
    // a stored text value is therefore not detectable by SQLite alone.
    // This limitation is documented in the Phase 6 research record and
    // factors into the encryption-at-rest decision (ADR-0026).
    //
    // The flip targets a byte INSIDE a known marker string's stored
    // bytes, so the test deterministically lands in cell payload rather
    // than a page header.
    let (_dir, db) = temp_db_path();
    const MARKER: &str = "payload-marker-UNIQUE-STRING-xyz";
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            MARKER,
            "--solution",
            "some solution text to fill pages",
            "--project",
            "p",
        ],
    );
    assert!(
        out.status.success(),
        "seed capture failed: {}",
        stderr(&out)
    );
    for i in 0..50 {
        let out = run(
            &db,
            &[
                "capture",
                "--problem",
                &format!("payload seed {i}"),
                "--solution",
                "some solution text to fill pages",
                "--project",
                "p",
            ],
        );
        assert!(
            out.status.success(),
            "seed capture failed: {}",
            stderr(&out)
        );
    }

    let mut bytes = std::fs::read(&db).expect("read db");
    let pos = bytes
        .windows(MARKER.len())
        .position(|w| w == MARKER.as_bytes())
        .expect("the marker text must be present in the database file");
    bytes[pos] ^= 0x01;
    let victim = db.with_extension("payload-flip.db");
    std::fs::write(&victim, &bytes).expect("write flipped db");

    // The database must still OPEN and pass the structural check — that
    // is exactly the documented limitation this test pins.
    let db = Db::open(&victim).expect("payload flip must not break structure");
    let report: String = db.with_connection(|c| {
        c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .expect("integrity check")
    });
    assert_eq!(report, "ok");
}

#[test]
fn healthy_database_after_crashes_remains_usable_by_the_cli() {
    // Sanity anchor for the crash tests: an untouched database passes
    // integrity_check and the search/list surface works end to end.
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "healthy database",
            "--solution",
            "s",
            "--project",
            "p",
        ],
    );
    assert!(out.status.success());
    assert_healthy(&db);
    let out = run(&db, &["search", "healthy database"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("problem:  healthy database"));
}
