//! CLI integration tests: the complete capture → database → search workflow
//! plus the required FTS behaviors (exact terms, multi-term, error-message
//! searches, ranking, ordering, malformed queries, no results).

mod common;

use common::{bin, run, stderr, stdout, temp_db_path};

const PROBLEM: &str = "PostgreSQL connection pool exhaustion on checkout-service";
const SOLUTION: &str = "Raised max_connections and enabled pgbouncer transaction pooling";

#[test]
fn capture_then_search_roundtrip() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            PROBLEM,
            "--solution",
            SOLUTION,
            "--project",
            "checkout-service",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    assert!(
        stdout(&out).contains("Captured #1"),
        "unexpected: {}",
        stdout(&out)
    );

    let out = run(&db, None, &["search", "postgres connection pool"], None);
    assert!(out.status.success(), "search failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("checkout-service"), "project missing: {text}");
    assert!(text.contains("pooling"), "solution missing: {text}");
    // Ranking data is explain-only; default output stays readable.
    assert!(
        !text.contains("fused"),
        "fused score must be hidden: {text}"
    );
    assert!(!text.contains('✓'), "piped output must stay plain: {text}");
    assert!(!text.contains('→'), "piped output must stay plain: {text}");

    // --explain exposes the per-engine ranking signals behind each hit.
    let out = run(
        &db,
        None,
        &["search", "--explain", "postgres connection pool"],
        None,
    );
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains("fused") && text.contains("fts_rank") && text.contains("semantic_sim"),
        "explain mode must show ranking signals: {text}"
    );
}

#[test]
fn list_shows_captured_entries() {
    let (_dir, db) = temp_db_path();
    run(
        &db,
        None,
        &["capture", "--problem", PROBLEM, "--solution", SOLUTION],
        None,
    );
    let out = run(&db, None, &["list"], None);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("#1"), "list should show entry id: {text}");
    assert!(
        text.contains("checkout"),
        "list should show problem: {text}"
    );
}

#[test]
fn piped_stdin_becomes_problem_without_flag() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &["capture", "--solution", "set busy_timeout to 5000ms"],
        Some("sqlite database is locked during migration\n"),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = run(&db, None, &["search", "database locked"], None);
    assert!(stdout(&out).contains("busy_timeout"), "solution not found");
}

#[test]
fn multi_line_stdin_capture_preserves_all_lines() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &[
            "capture",
            "--solution",
            "joined the writer into one transaction",
        ],
        Some("first line of the problem\nsecond line with more detail\n"),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = run(&db, None, &["search", "second line"], None);
    assert!(
        stdout(&out).contains("one transaction"),
        "multi-line problem must be stored and searchable"
    );
}

#[test]
fn duplicate_capture_is_skipped_and_force_overrides() {
    let (_dir, db) = temp_db_path();
    let flags = [
        "capture",
        "--problem",
        "sqlite database is locked",
        "--solution",
        "busy_timeout",
        "--project",
        "recall",
    ];
    let first = run(&db, None, &flags, None);
    assert!(first.status.success());
    assert!(stdout(&first).contains("Captured #1"));

    // Identical re-capture: deterministic skip, exit 0, store unchanged.
    let second = run(&db, None, &flags, None);
    assert!(
        second.status.success(),
        "dedup skip must exit 0: {}",
        stderr(&second)
    );
    let text = stdout(&second);
    assert!(text.contains("Skipped"), "expected skip message: {text}");
    assert!(
        text.contains("--force"),
        "skip message must mention --force: {text}"
    );

    // --force stores a second copy.
    let third = run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "sqlite database is locked",
            "--solution",
            "busy_timeout",
            "--project",
            "recall",
            "--force",
        ],
        None,
    );
    assert!(third.status.success());
    assert!(
        stdout(&third).contains("Captured #2"),
        "unexpected: {}",
        stdout(&third)
    );

    // The store now contains two memories; `list` shows the problems only,
    // so count via a search that matches both.
    let list = run(&db, None, &["list"], None);
    let list = stdout(&list);
    assert!(
        list.contains("#2 "),
        "second entry expected in list: {list}"
    );
    let hits = run(&db, None, &["search", "database locked"], None);
    let hits = stdout(&hits);
    assert_eq!(
        hits.matches("busy_timeout").count(),
        2,
        "two search hits expected: {hits}"
    );
}

#[test]
fn dedup_is_scoped_to_project_and_window() {
    let (_dir, db) = temp_db_path();
    // Same error, different project → both captured.
    let a = run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "p",
            "--solution",
            "s",
            "--error",
            "same error text",
            "--project",
            "project-a",
        ],
        None,
    );
    assert!(a.status.success());
    let b = run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "p",
            "--solution",
            "s",
            "--error",
            "same error text",
            "--project",
            "project-b",
        ],
        None,
    );
    assert!(b.status.success());
    assert!(
        stdout(&b).contains("Captured #2"),
        "different project must not dedup: {}",
        stdout(&b)
    );

    // Same problem (normalized) in the same project → skipped.
    let c = run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "P",
            "--solution",
            "s",
            "--project",
            "project-a",
        ],
        None,
    );
    assert!(c.status.success());
    assert!(
        stdout(&c).contains("Skipped"),
        "same normalized problem must dedup: {}",
        stdout(&c)
    );
}

#[test]
fn edit_updates_solution_and_fts_index() {
    let (_dir, db) = temp_db_path();
    run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "postgres pool exhausted",
            "--solution",
            "old fix",
            "--error",
            "too many clients",
        ],
        None,
    );

    let out = run(
        &db,
        None,
        &["edit", "1", "--solution", "raise the pool limit"],
        None,
    );
    assert!(out.status.success(), "edit failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Edited #1"));

    let search = run(&db, None, &["search", "postgres pool"], None);
    let text = stdout(&search);
    assert!(
        text.contains("raise the pool limit"),
        "edited solution must be found: {text}"
    );
    assert!(
        !text.contains("old fix"),
        "old solution must leave the FTS index: {text}"
    );
}

#[test]
fn edit_can_clear_optional_field() {
    let (_dir, db) = temp_db_path();
    run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "flakey ci",
            "--solution",
            "pin the action",
            "--error",
            "npm network timeout",
        ],
        None,
    );
    let out = run(&db, None, &["edit", "1", "--error", ""], None);
    assert!(out.status.success(), "edit failed: {}", stderr(&out));
    let search = run(&db, None, &["search", "npm network timeout"], None);
    assert!(
        stdout(&search).contains("No results"),
        "cleared error must leave the FTS index: {}",
        stdout(&search)
    );
}

#[test]
fn edit_missing_id_and_missing_flags_fail_cleanly() {
    let (_dir, db) = temp_db_path();
    run(
        &db,
        None,
        &["capture", "--problem", "p", "--solution", "s"],
        None,
    );

    let out = run(&db, None, &["edit", "999", "--solution", "x"], None);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("no memory with id 999"));

    let out = run(&db, None, &["edit", "1"], None);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("at least one field"));

    let out = run(&db, None, &["edit", "1", "--problem", ""], None);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("cannot be cleared"));
}

#[test]
fn explicit_stdin_flag_also_works() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &["capture", "--stdin", "--solution", "retry with backoff"],
        Some("TLS handshake timeout to payment-api\n"),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = run(&db, None, &["search", "TLS handshake"], None);
    assert!(stdout(&out).contains("backoff"), "solution not found");
}

#[test]
fn piped_stdin_requires_solution_flag() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, None, &["capture"], Some("problem from a pipe\n"));
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("--solution"),
        "stderr should point at --solution: {}",
        stderr(&out)
    );
}

#[test]
fn empty_problem_is_rejected() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &["capture", "--problem", "   ", "--solution", "fix"],
        None,
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("problem must not be empty"));
}

#[test]
fn empty_solution_is_rejected() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &["capture", "--problem", "problem", "--solution", ""],
        None,
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("solution must not be empty"));
}

#[test]
fn search_with_no_results_is_clear() {
    let (_dir, db) = temp_db_path();
    run(
        &db,
        None,
        &["capture", "--problem", PROBLEM, "--solution", SOLUTION],
        None,
    );
    let out = run(&db, None, &["search", "kafka consumer lag"], None);
    assert!(out.status.success(), "no-result search must exit 0");
    assert!(stdout(&out).contains("No results"));
}

#[test]
fn error_message_search_handles_special_characters() {
    let (_dir, db) = temp_db_path();
    run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "Missing table during report generation",
            "--solution",
            "re-ran the migration",
            "--error",
            r#"ERROR: relation "orders" does not exist (line 42)"#,
        ],
        None,
    );
    // Searching with the exact error fragment (quotes included) must work
    // and must not crash.
    let out = run(&db, None, &["search", r#"relation "orders""#], None);
    assert!(out.status.success(), "search failed: {}", stderr(&out));
    assert!(stdout(&out).contains("migration"), "solution not found");
}

#[test]
fn punctuation_only_query_is_rejected_cleanly() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, None, &["search", "*** :: ..."], None);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("searchable word"));
}

#[test]
fn error_field_ranks_higher_than_explanation() {
    let (_dir, db) = temp_db_path();
    // Entry 1: match only in the low-weight explanation field.
    run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "unrelated billing thing",
            "--solution",
            "whatever",
            "--explanation",
            "mentions postgres connection pool in passing",
        ],
        None,
    );
    // Entry 2: match in the high-weight error field.
    run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "API down at 3am",
            "--solution",
            "raised the pool limit",
            "--error",
            "postgres connection pool exhausted: too many clients",
        ],
        None,
    );
    let out = run(&db, None, &["search", "postgres connection pool"], None);
    assert!(out.status.success());
    let text = stdout(&out);
    // Entry 1 is identified by its problem text (the explanation field is
    // not part of the result display).
    let pos_error_entry = text.find("raised the pool limit").expect("entry 2 present");
    let pos_explanation_entry = text
        .find("unrelated billing thing")
        .expect("entry 1 present");
    assert!(
        pos_error_entry < pos_explanation_entry,
        "error-field match must rank above explanation-field match:\n{text}"
    );
}

#[test]
fn multiple_matches_are_all_returned() {
    let (_dir, db) = temp_db_path();
    for i in 1..=3 {
        run(
            &db,
            None,
            &[
                "capture",
                "--problem",
                &format!("kafka consumer lag on service-{i}"),
                "--solution",
                &format!("tuned fetch settings {i}"),
            ],
            None,
        );
    }
    let out = run(&db, None, &["search", "kafka consumer lag"], None);
    let text = stdout(&out);
    for i in 1..=3 {
        assert!(
            text.contains(&format!("service-{i}")),
            "missing match {i}: {text}"
        );
    }
}

#[test]
fn capture_works_with_all_optional_fields() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        None,
        &[
            "capture",
            "--problem",
            "flakey CI step",
            "--solution",
            "pinned the action version",
            "--error",
            "npm ERR! network timeout",
            "--context",
            "ubuntu-latest, node 22",
            "--investigation",
            "ran locally, checked runner logs",
            "--root-cause",
            "transitive dep bumped node engine",
            "--verification",
            "CI green on 3 runs",
            "--environment",
            "CI: ubuntu-latest",
            "--explanation",
            "always pin action SHAs",
            "--project",
            "ci-playground",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = run(&db, None, &["search", "npm network timeout"], None);
    assert!(stdout(&out).contains("pinned"), "solution not found");
}

#[test]
fn capture_is_atomic_enough_to_survive_immediate_search() {
    let (_dir, db) = temp_db_path();
    for i in 0..5 {
        let out = run(
            &db,
            None,
            &[
                "capture",
                "--problem",
                &format!("problem {i}"),
                "--solution",
                &format!("solution {i}"),
            ],
            None,
        );
        assert!(out.status.success());
        let out = run(&db, None, &["search", &format!("problem {i}")], None);
        assert!(stdout(&out).contains(&format!("solution {i}")));
    }
}

/// The binary is exercised through Cargo's test harness; keep a smoke
/// reference here so `bin()` is always used through the helper.
#[test]
fn binary_path_is_available() {
    assert!(std::path::Path::new(bin()).exists());
}
