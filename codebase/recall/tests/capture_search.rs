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
    assert!(text.contains("rank"), "rank missing: {text}");
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
