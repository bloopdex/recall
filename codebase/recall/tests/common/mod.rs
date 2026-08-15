//! Shared helpers for CLI integration tests.
//! All tests use temporary databases and directories — none depend on the
//! developer's personal repository state.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Path to the compiled `recall` binary (provided by Cargo for integration
/// tests via `CARGO_BIN_EXE_<name>`).
pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_recall")
}

/// Run `recall --db <db> <args>` in `cwd`, optionally piping `stdin_text`.
/// The embedding model is neutralized via `RECALL_MODEL_DIR` so CLI tests
/// stay fast and deterministic — real-model behavior is covered in
/// `tests/embeddings_integration.rs`.
#[allow(dead_code)]
pub fn run(db: &Path, cwd: Option<&Path>, args: &[&str], stdin_text: Option<&str>) -> Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--db").arg(db);
    cmd.args(args);
    // Point at a directory that never contains a model.
    cmd.env(
        "RECALL_MODEL_DIR",
        db.parent()
            .map(|p| p.join("no-model-dir"))
            .unwrap_or_else(|| std::path::PathBuf::from("no-model-dir")),
    );
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    match stdin_text {
        Some(text) => {
            cmd.stdin(Stdio::piped());
            // Capture stdout/stderr of the child as well — only stdin is
            // special here.
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

/// A temp directory containing a not-yet-created database file.
// Shared helper: not every integration-test target uses it.
#[allow(dead_code)]
pub fn temp_db_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("recall-test.db");
    (dir, db)
}

/// Like `run`, but with the real (default) model directory — used by the
/// real-model integration tests.
#[allow(dead_code)]
pub fn run_with_model(
    db: &Path,
    cwd: Option<&Path>,
    args: &[&str],
    stdin_text: Option<&str>,
) -> Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--db").arg(db);
    cmd.args(args);
    cmd.env_remove("RECALL_MODEL_DIR");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
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

// Shared helpers: not every integration-test target uses both.
#[allow(dead_code)]
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[allow(dead_code)]
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}
