//! CLI error contract (exit codes 0/1/2, ADR-0009).
//!
//! The exit-code contract, pinned by these tests:
//! - `0` — success (including "No results" and first-run database
//!   creation, which is a documented design choice, not an error)
//! - `1` — runtime error (message on stderr, actionable, never a panic)
//! - `2` — usage error (clap: missing/invalid arguments, unknown
//!   subcommand)
//!
//! Also covered: hostile input that must stay inert (SQL injection
//! attempts in filters are parameterized), very large inputs, and
//! unicode.

mod common;

use std::path::Path;
use std::process::Output;

use common::{stderr, stdout, temp_db_path};

const EXIT_USAGE: i32 = 2;

fn run(db: &Path, args: &[&str]) -> Output {
    common::run(db, None, args, None)
}

fn code(out: &Output) -> Option<i32> {
    out.status.code()
}

#[test]
fn missing_search_query_is_a_usage_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["search"]);
    assert_eq!(code(&out), Some(EXIT_USAGE));
    assert!(!stderr(&out).is_empty(), "usage errors explain themselves");
}

#[test]
fn non_numeric_id_is_a_usage_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["archive", "not-a-number"]);
    assert_eq!(code(&out), Some(EXIT_USAGE));
}

#[test]
fn missing_id_is_a_usage_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["archive"]);
    assert_eq!(code(&out), Some(EXIT_USAGE));
}

#[test]
fn delete_without_target_is_a_usage_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["delete"]);
    assert_eq!(code(&out), Some(EXIT_USAGE));
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["frobnicate"]);
    assert_eq!(code(&out), Some(EXIT_USAGE));
}

#[test]
fn operating_on_a_missing_id_is_a_runtime_error_not_a_panic() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["archive", "999"]);
    assert_eq!(code(&out), Some(1));
    let err = stderr(&out);
    assert!(err.contains("error"), "{err}");
    assert!(!err.contains("panicked"), "must never panic: {err}");
}

#[test]
fn first_run_on_a_fresh_path_creates_the_database_and_succeeds() {
    // Documented design: a missing database is created on first use —
    // search then simply has no results (exit 0, not an error).
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["search", "anything"]);
    assert_eq!(code(&out), Some(0));
    assert!(stdout(&out).contains("No results"));
    assert!(db.exists(), "first use must create the database");
}

#[test]
fn database_path_whose_parent_is_a_file_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "i am a file, not a directory").unwrap();
    let impossible = blocker.join("recall.db");
    let out = run(&impossible, &["list"]);
    assert_eq!(code(&out), Some(1));
    assert!(stderr(&out).contains("error"), "{}", stderr(&out));
}

#[test]
fn hostile_project_filter_is_inert() {
    // Filters are parameterized: a SQL-injection-shaped project name is
    // just a string that matches nothing.
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        &[
            "search",
            "--project",
            "x' OR 1=1; DROP TABLE memories; --",
            "anything",
        ],
    );
    assert_eq!(code(&out), Some(0));
    assert!(stdout(&out).contains("No results"), "{}", stdout(&out));
    // The database survived.
    let out = run(&db, &["check"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
}

#[test]
fn hostile_project_filter_on_delete_is_inert() {
    // Same principle on the lifecycle side: the project label is matched,
    // never executed. With nothing to delete, the documented contract is
    // a fail-closed runtime error ("no memories for project ...") — never
    // a SQL execution of the filter.
    let (_dir, db) = temp_db_path();
    let hostile = "x' OR 1=1; DROP TABLE memories; --";
    let out = run(&db, &["delete", "--project", hostile, "--yes"]);
    assert_eq!(code(&out), Some(1));
    assert!(
        stderr(&out).contains("no memories for project"),
        "{}",
        stderr(&out)
    );
    let out = run(&db, &["check"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
}

#[test]
fn unicode_problem_text_roundtrips_through_capture_and_search() {
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "запрос к пулу соединений превысил лимит 连接池 1000",
            "--solution",
            "поднял лимит 提高限制",
            "--project",
            "unicode",
        ],
    );
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let out = run(&db, &["search", "запрос соединений"]);
    assert_eq!(code(&out), Some(0));
    assert!(
        stdout(&out).contains("запрос к пулу соединений"),
        "unicode text must be searchable: {}",
        stdout(&out)
    );
}

#[test]
fn very_large_piped_input_is_captured_without_crashing() {
    // User-typed fields are deliberately not truncated (the ADR-0018
    // limits apply to AUTO-captured context only). A very large piped
    // problem must simply work.
    let (_dir, db) = temp_db_path();
    let big = "z".repeat(200_000);
    let stdin_text = format!("{big}\n");
    let out = common::run(
        &db,
        None,
        &["capture", "--stdin", "--solution", "big solution"],
        Some(&stdin_text),
    );
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let out = run(&db, &["list", "--limit", "1"]);
    assert!(stdout(&out).contains("zzzzzzzz"), "{}", stdout(&out));
}

#[test]
fn verbose_logging_goes_to_stderr_and_stdout_stays_data() {
    // stdout/stderr contract: search data on stdout; logs on stderr.
    let (_dir, db) = temp_db_path();
    let out = common::run(&db, None, &["--verbose", "search", "anything"], None);
    assert_eq!(code(&out), Some(0));
    assert!(stdout(&out).contains("No results"));
    // The verbose run emits a search event on stderr.
    assert!(stderr(&out).contains("search.run"), "{}", stderr(&out));
}

#[test]
fn logs_never_carry_memory_content_or_secrets() {
    // Log-data policy (see the observability module doc): tracing events
    // carry ids, counts, and metadata — never the captured text itself.
    // A secret in the problem must not appear in --verbose stderr.
    let (_dir, db) = temp_db_path();
    let out = common::run(
        &db,
        None,
        &[
            "--verbose",
            "capture",
            "--problem",
            "api key failure with sk_live_51hunter2secret",
            "--solution",
            "rotated the key",
        ],
        None,
    );
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("sk_live_51hunter2secret"),
        "logs must never contain the captured content: {}",
        stderr(&out)
    );
    assert!(
        !stderr(&out).contains("api key failure"),
        "logs must never carry memory text: {}",
        stderr(&out)
    );
}

#[test]
fn help_groups_commands_by_concept_and_stays_ascii() {
    let (_dir, db) = temp_db_path();
    let out = run(&db, &["--help"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    for group in [
        "Command groups:",
        "capture, search, list",
        "edit, archive, unarchive, delete",
        "projects, export, import",
        "shell, git",
        "check, embeddings, version",
        "RECALL_PLAIN",
    ] {
        assert!(text.contains(group), "help must document {group}: {text}");
    }
    // Help is decoration-free on every terminal: no icons, no arrows.
    for icon in ['✓', '✗', '⚠', '→', '🧠', '🔒', '📁'] {
        assert!(
            !text.contains(icon),
            "help must not carry icons ({icon}): {text}"
        );
    }
}

#[test]
fn first_run_stays_quiet_when_piped() {
    // The first-run welcome is an interactive-terminal experience only:
    // scripts and CI that create the database on first use must see
    // exactly the command's own output, no banner, no unicode.
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "first ever problem",
            "--solution",
            "first fix",
        ],
    );
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Captured #1"), "{text}");
    assert!(
        !text.contains("Local-first"),
        "banner leaked into piped output: {text}"
    );
    assert!(!text.contains("personal engineering memory"), "{text}");
    assert!(text.is_ascii(), "piped output must stay ASCII: {text}");
}

#[test]
fn plain_mode_is_the_default_for_pipes() {
    // Every command below is piped here; decorations must never appear.
    let (_dir, db) = temp_db_path();
    let out = run(
        &db,
        &[
            "capture",
            "--problem",
            "piped problem",
            "--solution",
            "piped fix",
        ],
    );
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    for args in [
        vec!["search", "piped problem"],
        vec!["list"],
        vec!["check"],
        vec!["version"],
        vec!["projects"],
    ] {
        let out = run(&db, &args);
        assert_eq!(code(&out), Some(0), "{args:?}: {}", stderr(&out));
        let text = stdout(&out);
        assert!(
            !text.contains('✓') && !text.contains('→') && !text.contains('🧠'),
            "{args:?} leaked decorations into piped output: {text}"
        );
    }
}
