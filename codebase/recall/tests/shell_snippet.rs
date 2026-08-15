//! Execute the generated shell snippets in REAL shells (ADR-0017).
//!
//! Each test is conditional on the shell being available on this machine
//! and skips otherwise, so the suite stays portable. PowerShell and Bash
//! are tested here (Git Bash on Windows); Zsh is generated but untested —
//! documented as such on the Phase 4 page.

mod common;

use std::path::Path;
use std::process::Command;

use common::{bin, temp_db_path};

fn shell_available(exe: &str, arg: &str) -> bool {
    Command::new(exe)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn write_snippet(dir: &Path, name: &str, snippet: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, snippet).unwrap();
    path
}

/// The snippet text comes from the library — the same bytes `recall shell
/// install` writes into startup files.
fn powershell_snippet() -> &'static str {
    recall::infrastructure::shell::powershell_snippet()
}

fn bash_snippet() -> &'static str {
    recall::infrastructure::shell::bash_snippet()
}

#[test]
fn powershell_snippet_records_a_failed_command_snapshot() {
    if !shell_available("powershell", "-NoProfile") {
        eprintln!("skipping: powershell.exe not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let snippet_path = write_snippet(dir.path(), "recall-snippet.ps1", powershell_snippet());
    let snippet_path = snippet_path.to_str().unwrap().replace('\\', "\\\\");

    // Dot-source the snippet, run a failing native command, then invoke
    // `prompt` directly (in a non-interactive session the prompt does not
    // render between commands — invoking it here is the deterministic
    // equivalent of what PowerShell does after every command). Command
    // history is injected via Add-History: interactive sessions record it
    // automatically, non-interactive ones do not.
    let script = format!(
        ". '{snippet_path}'\n\
         cmd /c exit 5\n\
         Add-History -InputObject 'cmd /c exit 5'\n\
         prompt\n\
         Write-Output \"CODE=$env:RECALL_LAST_EXIT_CODE\"\n\
         Write-Output \"CMD=$env:RECALL_LAST_COMMAND\"\n\
         Write-Output \"CWD=$env:RECALL_LAST_CWD\""
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "powershell failed: {text}");
    assert!(
        text.contains("CODE=5"),
        "failed command's exit code must be recorded: {text}"
    );
    assert!(
        text.contains("cmd /c exit 5"),
        "failed command line must be recorded: {text}"
    );
    assert!(
        text.contains(&format!("CWD={}", dir.path().to_string_lossy())),
        "cwd must be recorded: {text}"
    );
}

#[test]
fn powershell_snippet_clears_snapshot_after_success() {
    if !shell_available("powershell", "-NoProfile") {
        eprintln!("skipping: powershell.exe not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let snippet_path = write_snippet(dir.path(), "recall-snippet.ps1", powershell_snippet());
    let snippet_path = snippet_path.to_str().unwrap().replace('\\', "\\\\");
    let script = format!(
        ". '{snippet_path}'\n\
         cmd /c exit 5\n\
         prompt\n\
         Write-Output \"AFTER_FAIL=$env:RECALL_LAST_EXIT_CODE\"\n\
         cmd /c exit 0\n\
         prompt\n\
         Write-Output \"AFTER_OK=$env:RECALL_LAST_EXIT_CODE\""
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "powershell failed: {text}");
    assert!(text.contains("AFTER_FAIL=5"), "failure snapshot: {text}");
    assert!(
        text.contains("AFTER_OK="),
        "successful commands must clear the snapshot: {text}"
    );
    assert!(!text.contains("AFTER_OK=5"), "stale snapshot: {text}");
}

#[test]
fn powershell_recall_function_dispatches_search() {
    if !shell_available("powershell", "-NoProfile") {
        eprintln!("skipping: powershell.exe not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (_db_dir, db) = temp_db_path();

    // Seed one memory with a distinctive term.
    let out = Command::new(bin())
        .arg("--db")
        .arg(&db)
        .args([
            "capture",
            "--problem",
            "zzz-unique-shell-term connection pool",
            "--solution",
            "raised the pool size",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let snippet_path = write_snippet(dir.path(), "recall-snippet.ps1", powershell_snippet());
    let snippet_path = snippet_path.to_str().unwrap().replace('\\', "\\\\");
    let bin_dir = Path::new(bin())
        .parent()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "\\\\");
    let db_arg = db.to_string_lossy().replace('\\', "\\\\");
    let script = format!(
        ". '{snippet_path}'\n\
         $env:PATH = '{bin_dir};' + $env:PATH\n\
         $env:RECALL_DB_PATH = '{db_arg}'\n\
         $out = recall zzz-unique-shell-term\n\
         Write-Output $out"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "powershell failed: {text}");
    assert!(
        text.contains("zzz-unique-shell-term"),
        "the recall function must route unknown args to search: {text}"
    );
    assert!(
        text.contains("raised the pool size"),
        "search output must include the solution: {text}"
    );
}

#[test]
fn bash_snippet_records_the_last_exit_code() {
    if !shell_available("bash", "--version") {
        eprintln!("skipping: bash not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    write_snippet(dir.path(), "recall-snippet.sh", bash_snippet());
    // `false` fails with status 1; the PROMPT_COMMAND hook reads $? first.
    let script = "source recall-snippet.sh\nfalse\n__recall_capture_last\necho \"CODE=$RECALL_LAST_EXIT_CODE\"";
    let out = Command::new("bash")
        .args(["-c", script])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "bash failed: {text}");
    assert!(
        text.contains("CODE=1"),
        "the failed command's exit code must be recorded: {text}"
    );
}
