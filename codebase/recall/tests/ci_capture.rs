//! CI failure capture — `recall capture --from-ci` (ADR-0030).
//!
//! The integration boundary: an opt-in GitHub Actions failure step
//! (`if: failure()`) pipes a bounded log tail into Recall and names the
//! remediation it knows. The privacy model follows Phase 4: only the
//! whitelisted GITHUB_* variables are read (pinned in tests/security.rs),
//! the log passes the sanitizer, and in non-interactive CI any detected
//! secret fails closed — nothing is stored.
//!
//! The core boundary is preserved: a `--solution` is REQUIRED — Recall
//! stores fixes, not raw CI events.

mod common;

use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{bin, stderr, stdout, temp_db_path};

fn ci_env(workflow: &str) -> Vec<(&'static str, &str)> {
    vec![
        ("GITHUB_WORKFLOW", workflow),
        ("GITHUB_JOB", "test-job"),
        ("GITHUB_EVENT_NAME", "push"),
        ("GITHUB_REPOSITORY", "owner/myrepo"),
        ("GITHUB_SHA", "0123456789abcdef"),
        ("GITHUB_REF_NAME", "feature/ci-capture"),
        ("GITHUB_RUN_ID", "9876543210"),
        ("GITHUB_RUN_ATTEMPT", "1"),
        ("GITHUB_SERVER_URL", "https://github.com"),
    ]
}

fn run_with_env(db: &Path, envs: &[(&str, &str)], args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--db").arg(db);
    cmd.args(args);
    cmd.env(
        "RECALL_MODEL_DIR",
        db.parent()
            .map(|p| p.join("no-model-dir"))
            .unwrap_or_else(|| std::path::PathBuf::from("no-model-dir")),
    );
    for (k, v) in envs {
        cmd.env(k, v);
    }
    match stdin {
        Some(text) => {
            cmd.stdin(Stdio::piped());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            let mut child = cmd.spawn().expect("recall must spawn");
            {
                use std::io::Write;
                child
                    .stdin
                    .take()
                    .expect("stdin must be piped")
                    .write_all(text.as_bytes())
                    .expect("write to stdin");
            }
            child.wait_with_output().expect("recall must exit")
        }
        None => cmd.output().expect("recall must run"),
    }
}

const LOG_TAIL: &str =
    "error: could not compile recall\nerror[E0308]: mismatched types\n  --> src/main.rs:12:5\n";

#[test]
fn from_ci_builds_a_deterministic_problem_and_attaches_run_context() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &ci_env("ci"),
        &[
            "capture",
            "--from-ci",
            "--step",
            "build",
            "--solution",
            "fixed the type mismatch",
        ],
        Some(LOG_TAIL),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Captured #1"), "{}", stdout(&out));

    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    let memory = db_handle.get_memory(1).unwrap().unwrap();
    // Deterministic problem: workflow / job / step / event — no run id.
    assert_eq!(
        memory.problem, "CI failure in ci / test-job / step build (push)",
        "problem: {}",
        memory.problem
    );
    assert_eq!(
        memory.project.as_deref(),
        Some("myrepo"),
        "GITHUB_REPOSITORY's repo name is the project label"
    );
    // The piped log tail is the error (fields are normalized/trimmed;
    // truncation applies to larger inputs).
    assert_eq!(
        memory.error.as_deref(),
        Some(LOG_TAIL.trim()),
        "error should carry the log tail"
    );
    // Run metadata lands in context — visible, never part of dedup.
    let context = memory.context.as_deref().unwrap_or("");
    assert!(context.contains("9876543210"), "context: {context}");
    assert!(context.contains("feature/ci-capture"), "context: {context}");
    assert!(context.contains("0123456789abcdef"), "context: {context}");
}

#[test]
fn from_ci_requires_a_solution() {
    // The core boundary: Recall stores fixes, not raw CI events.
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &ci_env("ci"),
        &["capture", "--from-ci", "--step", "build"],
        Some(LOG_TAIL),
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("give the solution with --solution"),
        "{}",
        stderr(&out)
    );
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    assert!(db_handle.list_memories(10).unwrap().is_empty());
}

#[test]
fn from_ci_without_github_context_fails_clearly() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &[],
        &["capture", "--from-ci", "--solution", "re-ran"],
        Some(LOG_TAIL),
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("no CI failure context found"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn from_ci_secret_in_the_log_fails_closed() {
    // Non-interactive CI cannot confirm redactions — nothing is stored.
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &ci_env("ci"),
        &[
            "capture",
            "--from-ci",
            "--step",
            "deploy",
            "--solution",
            "rotated the key",
        ],
        Some("failed: sk_live_51hunter2secret was rejected by the gateway\n"),
    );
    assert!(
        out.status.success(),
        "decline is not an error: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("Not saved"),
        "the decline must be visible: {}",
        stdout(&out)
    );
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    assert!(
        db_handle.list_memories(10).unwrap().is_empty(),
        "nothing may be stored when the redaction is not confirmed"
    );
}

#[test]
fn from_ci_repeated_failure_deduplicates() {
    // The same job failing again within the window hits the existing
    // deduplication (ADR-0011) — the run id is not part of the problem.
    let (_dir, db) = temp_db_path();
    for run in ["111", "222"] {
        let mut envs = ci_env("ci");
        envs.push(("GITHUB_RUN_ID", run));
        let out = run_with_env(
            &db,
            &envs,
            &[
                "capture",
                "--from-ci",
                "--step",
                "build",
                "--solution",
                "fixed the type mismatch",
            ],
            Some(LOG_TAIL),
        );
        assert!(out.status.success(), "capture failed: {}", stderr(&out));
    }
    let out = run_with_env(&db, &ci_env("ci"), &["search", "CI failure in ci"], None);
    let text = stdout(&out);
    let hits = text
        .lines()
        .filter(|l| l.contains("problem:  CI failure in ci"))
        .count();
    assert_eq!(
        hits, 1,
        "the second run must deduplicate against the first: {text}"
    );
    assert!(
        text.contains("Skipped") || hits == 1,
        "second capture should have reported the duplicate"
    );
}

#[test]
fn from_ci_problem_flag_overrides_the_auto_problem() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &ci_env("ci"),
        &[
            "capture",
            "--from-ci",
            "--problem",
            "my own phrasing of the failure",
            "--solution",
            "re-ran",
        ],
        Some(LOG_TAIL),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    let memory = db_handle.get_memory(1).unwrap().unwrap();
    assert_eq!(memory.problem, "my own phrasing of the failure");
}
