//! Shell failure-context capture (ADR-0017/0018): the snapshot recorded by
//! the shell prompt hook arrives via the three whitelisted env vars, and
//! `recall capture --from-shell` turns it into a memory. The snippet itself
//! (hook behavior in real shells) is exercised in `shell_snippet.rs`; this
//! file covers the binary side end-to-end.

mod common;

use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{bin, stderr, stdout, temp_db_path};

const LAST_COMMAND: &str = "RECALL_LAST_COMMAND";
const LAST_EXIT_CODE: &str = "RECALL_LAST_EXIT_CODE";
const LAST_CWD: &str = "RECALL_LAST_CWD";

/// Run the binary with extra env vars and optional piped stdin.
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
    // Scrub the snapshot whitelist from the child's environment first:
    // tests in this binary run on parallel threads, and the sibling
    // secret-redaction test mutates these process-global vars in-process.
    // A spawn during that window (or an ambient var on the runner) must
    // never leak a snapshot into a test that did not provide one - each
    // child observes exactly the context its test declares, including
    // the "no context at all" case.
    for var in [LAST_COMMAND, LAST_EXIT_CODE, LAST_CWD] {
        cmd.env_remove(var);
    }
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

fn snapshot<'a>(command: &'a str, exit: &'a str) -> Vec<(&'static str, &'a str)> {
    vec![
        (LAST_COMMAND, command),
        (LAST_EXIT_CODE, exit),
        (LAST_CWD, "C:\\work"),
    ]
}

#[test]
fn from_shell_prefills_problem_with_command_and_exit_code() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &snapshot("cargo test --release", "101"),
        &[
            "capture",
            "--from-shell",
            "--solution",
            "ran with --features downloads",
        ],
        Some(""),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Captured #1"), "{}", stdout(&out));

    let out = run_with_env(&db, &[], &["search", "cargo test release"], None);
    let text = stdout(&out);
    assert!(
        text.contains("exit code 101"),
        "problem should carry the exit code: {text}"
    );
    assert!(
        text.contains("cargo test --release"),
        "problem should carry the command: {text}"
    );
}

#[test]
fn from_shell_without_snapshot_errors_clearly() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &[],
        &["capture", "--from-shell", "--solution", "x"],
        None,
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("no shell failure context"),
        "unexpected error: {}",
        stderr(&out)
    );
}

#[test]
fn from_shell_piped_stdin_becomes_the_error_text() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &snapshot("npm test", "1"),
        &["capture", "--from-shell", "--solution", "fixed the flake"],
        Some("ERROR: relation \"orders\" does not exist (line 42)\n"),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));

    let out = run_with_env(&db, &[], &["search", "orders does not exist"], None);
    let text = stdout(&out);
    assert!(
        text.contains("line 42"),
        "the piped error text should be stored: {text}"
    );
    assert!(text.contains("npm test"), "command prefill missing: {text}");
}

#[test]
fn from_shell_secret_is_redacted_after_interactive_confirmation() {
    // The interactive path (TTY): the redacted prefill is shown and saved
    // only after an explicit "yes". Driven at the library boundary because
    // a spawned binary cannot fake a terminal.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("recall-test.db");
    let mut db = recall::infrastructure::database::Db::open(&db_path).unwrap();

    std::env::set_var("RECALL_LAST_COMMAND", "npm login --password hunter2");
    std::env::set_var("RECALL_LAST_EXIT_CODE", "1");
    std::env::remove_var("RECALL_LAST_CWD");

    let args = recall::cli::CaptureArgs {
        from_shell: true,
        solution: Some("switched to a token".into()),
        ..Default::default()
    };
    // TTY input: "y" answers the redaction confirmation, then an empty
    // line accepts the prefill; the solution comes from the flag.
    let outcome = recall::application::capture::run_with_io(
        &mut db,
        &args,
        dir.path(),
        true,
        &mut std::io::Cursor::new(b"y\n\n".to_vec()),
        &mut Vec::new(),
    )
    .unwrap();
    std::env::remove_var("RECALL_LAST_COMMAND");
    std::env::remove_var("RECALL_LAST_EXIT_CODE");

    match outcome {
        recall::application::capture::CaptureOutcome::Captured { id, .. } => {
            let memory = db.get_memory(id).unwrap().unwrap();
            assert!(
                memory.problem.contains("<redacted>"),
                "the secret must be redacted: {}",
                memory.problem
            );
            assert!(
                !memory.problem.contains("hunter2"),
                "the secret must never be stored: {}",
                memory.problem
            );
        }
        other => panic!("expected Captured, got {other:?}"),
    }
}

#[test]
fn from_shell_secret_with_piped_input_fails_closed() {
    // With piped stdin there is no terminal to read the confirmation from
    // (the pipe is consumed as error text). Privacy wins: capture is
    // declined rather than silently storing auto-captured secrets.
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &snapshot("gh auth --token=ghp_abc123", "1"),
        &["capture", "--from-shell", "--solution", "x"],
        Some("some error output from the failed command\n"),
    );
    assert!(out.status.success(), "declining is not an error");
    assert!(
        stdout(&out).contains("Not saved"),
        "unexpected output: {}",
        stdout(&out)
    );

    let out = run_with_env(&db, &[], &["search", "gh auth"], None);
    assert!(stdout(&out).contains("No results"));
}

#[test]
fn from_shell_without_solution_flag_fails_clearly_when_piping() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &snapshot("make test", "2"),
        &["capture", "--from-shell"],
        Some("make: *** [test] Error 2\n"),
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("--solution"),
        "unexpected error: {}",
        stderr(&out)
    );
}

#[test]
fn from_shell_non_numeric_exit_code_degrades_to_unknown() {
    let (_dir, db) = temp_db_path();
    let out = run_with_env(
        &db,
        &snapshot("make test", "not-a-number"),
        &["capture", "--from-shell", "--solution", "fixed"],
        Some(""),
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = run_with_env(&db, &[], &["search", "make test"], None);
    assert!(stdout(&out).contains("exit code unknown"));
}

#[test]
fn from_shell_dedup_skips_repeated_failures() {
    let (_dir, db) = temp_db_path();
    let snap = snapshot("flakey-ci --retries 1", "3");
    let out = run_with_env(
        &db,
        &snap,
        &["capture", "--from-shell", "--solution", "pinned the runner"],
        Some(""),
    );
    assert!(out.status.success());
    assert!(stdout(&out).contains("Captured #1"));

    let out = run_with_env(
        &db,
        &snap,
        &["capture", "--from-shell", "--solution", "pinned the runner"],
        Some(""),
    );
    assert!(out.status.success(), "dedup skip is not an error");
    assert!(
        stdout(&out).contains("Skipped: near-identical"),
        "expected dedup skip, got: {}",
        stdout(&out)
    );
}
