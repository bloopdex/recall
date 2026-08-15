//! Git hook lifecycle and `capture --from-git` (ADR-0019/0020).
//!
//! All repositories are temporary — none depend on personal repo state.
//! The reliability boundary is structural: the hook runs AFTER the commit,
//! so even a Recall crash cannot abort a commit; these tests pin that
//! behavior plus preservation of user hooks, worktree handling, and bare
//! repositories.

mod common;

use std::path::{Path, PathBuf};
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

fn hook_file(repo: &Path) -> PathBuf {
    repo.join(".git").join("hooks").join("post-commit")
}

fn run_git_subcommand(repo: &Path, db: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--db").arg(db);
    cmd.args(args);
    cmd.current_dir(repo);
    cmd.output().expect("recall must run")
}

#[test]
fn install_creates_marked_hook_and_uninstall_removes_it() {
    let repo = init_repo();
    let (_dir, db) = temp_db_path();

    let out = run_git_subcommand(repo.path(), &db, &["git", "install"]);
    assert!(out.status.success(), "install failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Installed"), "{}", stdout(&out));

    let hook = std::fs::read_to_string(hook_file(repo.path())).unwrap();
    assert!(hook.starts_with("#!/bin/sh"), "hook needs a shebang");
    assert!(hook.contains("# >>> recall git hook >>>"));
    assert!(hook.contains("# <<< recall git hook <<<"));
    assert!(hook.contains("recall capture --from-git"));

    let out = run_git_subcommand(repo.path(), &db, &["git", "status"]);
    assert!(stdout(&out).contains("installed"), "{}", stdout(&out));

    let out = run_git_subcommand(repo.path(), &db, &["git", "uninstall"]);
    assert!(out.status.success());
    assert!(
        !hook_file(repo.path()).exists(),
        "recall-installed hook should be deleted"
    );

    let out = run_git_subcommand(repo.path(), &db, &["git", "status"]);
    assert!(stdout(&out).contains("not installed"), "{}", stdout(&out));
}

#[test]
fn install_is_idempotent() {
    let repo = init_repo();
    let (_dir, db) = temp_db_path();
    run_git_subcommand(repo.path(), &db, &["git", "install"]);
    let out = run_git_subcommand(repo.path(), &db, &["git", "install"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("Already installed"),
        "{}",
        stdout(&out)
    );
    let hook = std::fs::read_to_string(hook_file(repo.path())).unwrap();
    assert_eq!(hook.matches(">>> recall git hook >>>").count(), 1);
}

#[test]
fn existing_user_hook_is_refused_and_append_preserves_it() {
    let repo = init_repo();
    let (_dir, db) = temp_db_path();
    std::fs::write(
        hook_file(repo.path()),
        "#!/bin/sh\n# my own hook\necho user-hook-content\n",
    )
    .unwrap();

    // Refused without --append: the user's hook is untouched.
    let out = run_git_subcommand(repo.path(), &db, &["git", "install"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("existing post-commit hook"),
        "unexpected error: {}",
        stderr(&out)
    );
    let content = std::fs::read_to_string(hook_file(repo.path())).unwrap();
    assert!(
        !content.contains("recall"),
        "user hook must not be modified"
    );

    // --append adds the recall block after the user's content.
    let out = run_git_subcommand(repo.path(), &db, &["git", "install", "--append"]);
    assert!(out.status.success(), "append failed: {}", stderr(&out));
    let content = std::fs::read_to_string(hook_file(repo.path())).unwrap();
    assert!(content.contains("# my own hook"));
    assert!(content.contains("echo user-hook-content"));
    assert!(content.contains("recall capture --from-git"));

    // Uninstall removes only the recall block.
    let out = run_git_subcommand(repo.path(), &db, &["git", "uninstall"]);
    assert!(out.status.success());
    let content = std::fs::read_to_string(hook_file(repo.path())).unwrap();
    assert!(!content.contains("recall"), "recall block must be gone");
    assert!(
        content.contains("echo user-hook-content"),
        "user hook content must survive uninstall: {content}"
    );
}

#[test]
fn worktree_install_lands_in_the_common_hooks_dir() {
    let repo = init_repo();
    git(repo.path(), &["commit", "--allow-empty", "-m", "init"]);
    let wt_dir = tempfile::tempdir().unwrap();
    let wt = wt_dir.path().join("worktree");
    git(
        repo.path(),
        &[
            "worktree",
            "add",
            wt.to_str().unwrap(),
            "-b",
            "feature-branch",
        ],
    );

    let (_dir, db) = temp_db_path();
    let out = run_git_subcommand(&wt, &db, &["git", "install"]);
    assert!(out.status.success(), "install failed: {}", stderr(&out));
    // `git rev-parse --git-path hooks` in a worktree resolves to the
    // COMMON hooks dir of the main repository.
    assert!(
        hook_file(repo.path()).exists(),
        "hook must land in the shared hooks dir, not the worktree's"
    );
}

#[test]
fn bare_repository_install_is_refused() {
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare"]);
    let (_dir, db) = temp_db_path();
    let out = run_git_subcommand(bare.path(), &db, &["git", "install"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("bare"),
        "unexpected: {}",
        stderr(&out)
    );
}

#[test]
fn non_repo_status_reports_not_applicable() {
    let dir = tempfile::tempdir().unwrap();
    let (_dir, db) = temp_db_path();
    let out = run_git_subcommand(dir.path(), &db, &["git", "status"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("not applicable"), "{}", stdout(&out));
}

#[test]
fn commit_succeeds_even_when_hook_finds_no_recall_binary() {
    // The hard reliability boundary: with the hook installed and `recall`
    // NOT on PATH, `git commit` must behave exactly as before.
    let repo = init_repo();
    let (_dir, db) = temp_db_path();
    run_git_subcommand(repo.path(), &db, &["git", "install"]);

    std::fs::write(repo.path().join("file.txt"), "content").unwrap();
    git(repo.path(), &["add", "file.txt"]);
    // Strip the test binary's directory from PATH so the hook's
    // `command -v recall` fails (the real no-recall-installed case).
    let bin_dir = Path::new(bin()).parent().unwrap();
    let mut cmd = Command::new("git");
    cmd.current_dir(repo.path());
    cmd.args(["commit", "-m", "fix: works without recall"]);
    let stripped_path = std::env::var("PATH")
        .unwrap_or_default()
        .split(';')
        .filter(|p| !Path::new(p).starts_with(bin_dir))
        .collect::<Vec<_>>()
        .join(";");
    cmd.env("PATH", stripped_path);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "commit must succeed without recall: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn hook_skips_silently_without_a_terminal() {
    // Run the installed hook script the way git would (via sh), with
    // recall on PATH and stdin not a terminal: it must skip, exit 0, and
    // never block.
    let sh_ok = Command::new("sh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !sh_ok {
        eprintln!("skipping: `sh` not available in this environment");
        return;
    }
    let repo = init_repo();
    let (_dir, db) = temp_db_path();
    run_git_subcommand(repo.path(), &db, &["git", "install"]);

    let bin_dir = Path::new(bin())
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let mut path = format!("{bin_dir};");
    path.push_str(&std::env::var("PATH").unwrap_or_default());
    let out = Command::new("sh")
        .current_dir(repo.path())
        .arg(hook_file(repo.path()).to_str().unwrap())
        .env("PATH", path)
        .env(
            "RECALL_MODEL_DIR",
            db.parent().unwrap().join("no-model-dir"),
        )
        .env("RECALL_DB_PATH", &db)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "non-interactive hook must exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout(&out).contains("no interactive terminal"),
        "expected the skip message, got: {}",
        stdout(&out)
    );
}

#[test]
fn capture_from_git_records_commit_subject_and_files() {
    let repo = init_repo();
    std::fs::write(repo.path().join("pool.rs"), "fix").unwrap();
    git(repo.path(), &["add", "pool.rs"]);
    git(
        repo.path(),
        &["commit", "-m", "fix: race condition in pool"],
    );

    let (_dir, db) = temp_db_path();
    let mut cmd = Command::new(bin());
    cmd.arg("--db").arg(&db);
    cmd.args(["capture", "--from-git", "--solution", "added a mutex"]);
    cmd.current_dir(repo.path());
    cmd.env(
        "RECALL_MODEL_DIR",
        db.parent().unwrap().join("no-model-dir"),
    );
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
    assert!(stdout(&out).contains("Captured #1"), "{}", stdout(&out));

    let out = run_git_subcommand(repo.path(), &db, &["search", "race condition pool"]);
    let text = stdout(&out);
    assert!(
        text.contains("fix: race condition in pool"),
        "problem should carry the commit subject: {text}"
    );

    // The commit's changed files live in git_changed_files (the search
    // display deliberately does not print that field) — read the memory.
    let db_handle = recall::infrastructure::database::Db::open(&db).unwrap();
    let memory = db_handle.get_memory(1).unwrap().unwrap();
    assert!(
        memory.git_commit.is_some(),
        "the commit SHA must be recorded"
    );
    assert!(
        memory
            .git_changed_files
            .as_deref()
            .unwrap_or("")
            .contains("pool.rs"),
        "changed files of the commit should be stored: {:?}",
        memory.git_changed_files
    );
}

#[test]
fn from_git_without_terminal_and_without_input_skips() {
    let repo = init_repo();
    git(
        repo.path(),
        &["commit", "--allow-empty", "-m", "fix: empty"],
    );
    let (_dir, db) = temp_db_path();
    let out = run_git_subcommand(repo.path(), &db, &["capture", "--from-git"]);
    assert!(out.status.success(), "skipping must not be an error");
    assert!(
        stdout(&out).contains("no interactive terminal"),
        "unexpected: {}",
        stdout(&out)
    );
}

// ---------------------------------------------------------------------------
// Library-level tests for the interactive (TTY) prompt path of --from-git:
// the binary can't fake a terminal, so the flow is driven through
// `capture::run_with_io` with injected streams.
// ---------------------------------------------------------------------------

fn open_db(db_path: &Path) -> recall::infrastructure::database::Db {
    recall::infrastructure::database::Db::open(db_path).expect("db must open")
}

fn tty_args() -> recall::cli::CaptureArgs {
    recall::cli::CaptureArgs {
        from_git: true,
        ..Default::default()
    }
}

fn input_lines(text: &str) -> std::io::Cursor<Vec<u8>> {
    std::io::Cursor::new(text.as_bytes().to_vec())
}

#[test]
fn from_git_tty_flow_prefills_and_captures() {
    let repo = init_repo();
    std::fs::write(repo.path().join("a.rs"), "x").unwrap();
    git(repo.path(), &["add", "a.rs"]);
    git(repo.path(), &["commit", "-m", "fix: deadlock on startup"]);

    let (_dir, db_path) = temp_db_path();
    let mut db = open_db(&db_path);
    let args = tty_args();
    // TTY input: Enter accepts the prefill, then type the solution.
    let outcome = recall::application::capture::run_with_io(
        &mut db,
        &args,
        repo.path(),
        true,
        &mut input_lines("\nused a timeout instead of a lock\n"),
        &mut Vec::new(),
    )
    .unwrap();
    match outcome {
        recall::application::capture::CaptureOutcome::Captured { id, .. } => {
            let memory = db.get_memory(id).unwrap().unwrap();
            assert!(
                memory.problem.contains("fix: deadlock on startup"),
                "problem: {}",
                memory.problem
            );
            assert!(
                memory.solution.contains("timeout"),
                "solution: {}",
                memory.solution
            );
            assert!(memory.git_commit.is_some());
            assert!(memory
                .git_changed_files
                .as_deref()
                .unwrap_or("")
                .contains("a.rs"));
        }
        other => panic!("expected Captured, got {other:?}"),
    }
}

#[test]
fn from_git_tty_flow_can_be_skipped() {
    let repo = init_repo();
    git(
        repo.path(),
        &["commit", "--allow-empty", "-m", "fix: whatever"],
    );
    let (_dir, db_path) = temp_db_path();
    let mut db = open_db(&db_path);
    let args = tty_args();
    let outcome = recall::application::capture::run_with_io(
        &mut db,
        &args,
        repo.path(),
        true,
        &mut input_lines("skip\n"),
        &mut Vec::new(),
    )
    .unwrap();
    assert!(matches!(
        outcome,
        recall::application::capture::CaptureOutcome::Declined { .. }
    ));
}

#[test]
fn from_git_non_tty_without_flags_declines() {
    let repo = init_repo();
    git(
        repo.path(),
        &["commit", "--allow-empty", "-m", "fix: whatever"],
    );
    let (_dir, db_path) = temp_db_path();
    let mut db = open_db(&db_path);
    let args = tty_args();
    let outcome = recall::application::capture::run_with_io(
        &mut db,
        &args,
        repo.path(),
        false,
        &mut input_lines(""),
        &mut Vec::new(),
    )
    .unwrap();
    assert!(matches!(
        outcome,
        recall::application::capture::CaptureOutcome::Declined { .. }
    ));
}
