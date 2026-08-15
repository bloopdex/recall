//! Shell / Git integration hardening (Phase 6).
//!
//! The hard rule (ADR-0017/0019/0020): Recall must never break the
//! user's shell or Git workflow. The Phase 4 suites already pin hook
//! preservation, bare-repo refusal, the missing-binary commit path, and
//! the non-TTY skip. This file adds the remaining edges:
//! - uninstall idempotency (repeat calls never error)
//! - a very large shell snapshot is truncated, not stored whole
//! - malformed snapshot state (missing exit code / cwd) degrades safely

mod common;

use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{bin, stderr, stdout, temp_db_path};

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("git must be available for these tests");
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "test"]);
    dir
}

fn run_git_subcommand(repo: &Path, db: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--db").arg(db);
    cmd.args(args);
    cmd.current_dir(repo);
    cmd.output().expect("recall must run")
}

fn run_with_env(
    db: &Path,
    envs: &[(&str, &str)],
    args: &[&str],
    stdin_text: Option<&str>,
) -> Output {
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
    match stdin_text {
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

#[test]
fn git_uninstall_is_idempotent() {
    let repo = init_repo();
    let (_dir, db) = temp_db_path();
    run_git_subcommand(repo.path(), &db, &["git", "install"]);

    let out = run_git_subcommand(repo.path(), &db, &["git", "uninstall"]);
    assert!(
        out.status.success(),
        "first uninstall failed: {}",
        stderr(&out)
    );
    assert!(stdout(&out).contains("Uninstalled"), "{}", stdout(&out));

    // A second uninstall is not an error — nothing to remove.
    let out = run_git_subcommand(repo.path(), &db, &["git", "uninstall"]);
    assert!(
        out.status.success(),
        "second uninstall failed: {}",
        stderr(&out)
    );
    assert!(stdout(&out).contains("Not installed"), "{}", stdout(&out));
}

#[test]
fn a_very_large_shell_snapshot_is_truncated_before_storage() {
    // ADR-0018: auto-captured command lines are capped at 1000 chars. A
    // 5000-char command in the snapshot must arrive in the store
    // truncated, with the marker.
    let (_dir, db) = temp_db_path();
    let long_command = format!("tool --flag={}", "x".repeat(5000));
    let out = run_with_env(
        &db,
        &[
            ("RECALL_LAST_COMMAND", &long_command),
            ("RECALL_LAST_EXIT_CODE", "1"),
            ("RECALL_LAST_CWD", "C:\\work"),
        ],
        &[
            "capture",
            "--from-shell",
            "--solution",
            "fixed the long command",
        ],
        Some(""),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));

    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    let memory = db_handle.get_memory(1).unwrap().unwrap();
    assert!(
        memory.problem.len() < 2000,
        "the stored problem must be truncated, got {} chars",
        memory.problem.len()
    );
    assert!(
        memory.problem.ends_with("... (truncated)"),
        "the truncation marker must survive: {}",
        memory.problem
    );
    assert!(
        !memory.problem.contains(&"x".repeat(2000)),
        "the full command must not be stored"
    );
}

#[test]
fn malformed_snapshot_state_degrades_safely() {
    // Only the command is present: the exit code and cwd fall back to
    // documented defaults instead of failing the capture.
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &[("RECALL_LAST_COMMAND", "broken command")],
        &["capture", "--from-shell", "--solution", "fixed it"],
        Some(""),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    let memory = db_handle.get_memory(1).unwrap().unwrap();
    assert!(
        memory.problem.contains("broken command"),
        "problem: {}",
        memory.problem
    );
    assert!(
        memory.problem.contains("exit code"),
        "the defaulted exit code must be recorded: {}",
        memory.problem
    );
}

#[test]
fn shell_snapshot_with_secret_requires_confirmation_even_in_piping() {
    // The secret gate is not bypassed by non-TTY input: a secret in the
    // snapshot fails closed (decline) when the piped answer is not an
    // explicit yes.
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &[
            ("RECALL_LAST_COMMAND", "deploy --password hunter2"),
            ("RECALL_LAST_EXIT_CODE", "1"),
            ("RECALL_LAST_CWD", "C:\\work"),
        ],
        &["capture", "--from-shell", "--solution", "fixed it"],
        Some("n\n"),
    );
    assert!(
        out.status.success(),
        "decline is not an error: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("Not saved"),
        "the decline reason must be printed: {}",
        stdout(&out)
    );
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    assert!(
        db_handle.list_memories(10).unwrap().is_empty(),
        "nothing may be stored when the redaction is declined"
    );
}
