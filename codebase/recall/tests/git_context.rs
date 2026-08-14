//! CLI + library tests for git/project context capture: inside a repo,
//! outside git, detached HEAD, empty repo, and a missing git executable.

mod common;

use std::path::Path;
use std::process::Command;

use common::{run, stderr, stdout, temp_db_path};

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("git must be available for these tests");
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

fn init_repo(dir: &Path) -> String {
    let repo = dir.join("thorn-api");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "test"]);
    std::fs::write(repo.join("src.txt"), "content").unwrap();
    git(&repo, &["add", "src.txt"]);
    git(&repo, &["commit", "-m", "init"]);
    let short = Command::new("git")
        .current_dir(&repo)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    let short = String::from_utf8(short.stdout).unwrap().trim().to_string();
    assert!(!short.is_empty());
    short
}

#[test]
fn capture_inside_git_repo_records_project_and_commit() {
    let dir = tempfile::tempdir().unwrap();
    let (db_dir, db) = temp_db_path();
    let _ = db_dir;
    let short_sha = init_repo(dir.path());
    let repo = dir.path().join("thorn-api");

    let out = run(
        &db,
        Some(&repo),
        &[
            "capture",
            "--problem",
            "postgres pool exhausted",
            "--solution",
            "raised the limit",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));

    let out = run(&db, None, &["list"], None);
    let text = stdout(&out);
    assert!(
        text.contains("thorn-api"),
        "project must be detected: {text}"
    );
    assert!(text.contains(&short_sha), "commit must be captured: {text}");
}

#[test]
fn capture_outside_git_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let (_db_dir, db) = temp_db_path();
    let plain = dir.path().join("no-git-here");
    std::fs::create_dir_all(&plain).unwrap();

    let out = run(
        &db,
        Some(&plain),
        &[
            "capture",
            "--problem",
            "works on my machine",
            "--solution",
            "found the drift",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    let out = run(&db, None, &["search", "drift"], None);
    assert!(stdout(&out).contains("found the drift"));
}

#[test]
fn capture_in_detached_head_works() {
    let dir = tempfile::tempdir().unwrap();
    let (_db_dir, db) = temp_db_path();
    init_repo(dir.path());
    let repo = dir.path().join("thorn-api");
    git(&repo, &["checkout", "--detach"]);

    let out = run(
        &db,
        Some(&repo),
        &[
            "capture",
            "--problem",
            "detached state issue",
            "--solution",
            "reattach",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
}

#[test]
fn capture_in_empty_repo_works() {
    let dir = tempfile::tempdir().unwrap();
    let (_db_dir, db) = temp_db_path();
    let repo = dir.path().join("empty");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);

    let out = run(
        &db,
        Some(&repo),
        &[
            "capture",
            "--problem",
            "empty repo problem",
            "--solution",
            "first commit",
        ],
        None,
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
}

#[test]
fn capture_with_git_missing_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let (_db_dir, db) = temp_db_path();
    // A PATH containing only an empty directory: `git` cannot be found.
    let empty_path_dir = dir.path().join("empty-path");
    std::fs::create_dir_all(&empty_path_dir).unwrap();
    let path_var = std::env::var_os("PATH").expect("PATH exists");
    // Keep system dirs so the OS loader still works; remove git by
    // prepending an empty dir does NOT remove git. Instead run with a
    // PATH that has only the empty dir — `Command::new("git")` then fails.
    let _ = path_var;

    let mut cmd = Command::new(common::bin());
    cmd.arg("--db").arg(&db);
    cmd.args([
        "capture",
        "--problem",
        "no git around",
        "--solution",
        "fine",
    ]);
    cmd.current_dir(dir.path());
    cmd.env("PATH", &empty_path_dir);
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "capture must not require git: {}",
        stderr(&out)
    );
}

#[test]
fn explicit_project_flag_overrides_detection() {
    let dir = tempfile::tempdir().unwrap();
    let (_db_dir, db) = temp_db_path();
    let plain = dir.path().join("plain-dir");
    std::fs::create_dir_all(&plain).unwrap();

    run(
        &db,
        Some(&plain),
        &[
            "capture",
            "--problem",
            "override check",
            "--solution",
            "done",
            "--project",
            "my-custom-project",
        ],
        None,
    );
    let out = run(&db, None, &["list"], None);
    assert!(stdout(&out).contains("my-custom-project"));
}
