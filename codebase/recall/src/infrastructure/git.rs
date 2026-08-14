//! Best-effort git metadata capture.
//!
//! Strategy (ADR-008): spawn the `git` executable with fixed argument
//! vectors — never a shell, never user-provided strings — and treat every
//! failure (missing binary, not a repo, detached HEAD, empty repo) as
//! "metadata unavailable". Git metadata is never a requirement for capture.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Git context captured at capture time. All fields optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitContext {
    /// Repository root (top-level directory of the enclosing repo).
    pub repo_root: Option<PathBuf>,
    /// Current branch; `None` when detached or undeterminable.
    pub branch: Option<String>,
    /// Short current commit SHA; `None` in an empty repository.
    pub commit: Option<String>,
    /// Changed files from `git status --porcelain`, capped at 50 lines.
    pub changed_files: Option<String>,
}

impl GitContext {
    /// Detect git context from `cwd`. Never fails: a non-git directory or
    /// a missing git executable simply yields `GitContext::default()`.
    pub fn detect(cwd: &Path) -> Self {
        let mut ctx = GitContext::default();

        let Some(root) = git_output(cwd, &["rev-parse", "--show-toplevel"]) else {
            return ctx;
        };
        ctx.repo_root = Some(PathBuf::from(root.trim()));

        if let Some(branch) = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]) {
            let branch = branch.trim().to_string();
            // "HEAD" is git's marker for a detached state.
            if branch != "HEAD" && !branch.is_empty() {
                ctx.branch = Some(branch);
            }
        }

        if let Some(commit) = git_output(cwd, &["rev-parse", "--short", "HEAD"]) {
            let commit = commit.trim().to_string();
            if !commit.is_empty() {
                ctx.commit = Some(commit);
            }
        }

        if let Some(status) = git_output(cwd, &["status", "--porcelain"]) {
            let files: Vec<&str> = status.lines().take(50).collect();
            if !files.is_empty() {
                ctx.changed_files = Some(files.join("\n"));
            }
        }

        ctx
    }
}

/// Detect the project name: the repository's top-level directory name when
/// inside git, otherwise the current directory name.
pub fn detect_project(cwd: &Path, git: &GitContext) -> Option<String> {
    let base = git
        .repo_root
        .as_deref()
        .or(Some(cwd))
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().to_string());
    match base {
        Some(b) if !b.is_empty() => Some(b),
        _ => None,
    }
}

/// Run `git <args>` in `cwd` and return trimmed stdout. `None` on any
/// failure: missing executable, non-zero exit, or non-UTF-8 output.
fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .status()
            .expect("git must be available for these tests");
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn non_git_directory_yields_empty_context() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = GitContext::detect(dir.path());
        assert_eq!(ctx, GitContext::default());
        assert_eq!(
            detect_project(dir.path(), &ctx).as_deref(),
            Some(dir.path().file_name().unwrap().to_str().unwrap())
        );
    }

    #[test]
    fn git_repo_yields_root_branch_commit_and_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("my-repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "test"]);
        std::fs::write(repo.join("a.txt"), "hello").unwrap();
        git(&repo, &["add", "a.txt"]);
        git(&repo, &["commit", "-m", "init"]);
        std::fs::write(repo.join("b.txt"), "uncommitted").unwrap();

        let ctx = GitContext::detect(&repo);
        assert_eq!(ctx.repo_root.as_deref(), Some(repo.as_path()));
        assert_eq!(ctx.branch.as_deref(), Some("main"));
        assert!(ctx.commit.is_some(), "commit SHA should be captured");
        assert!(ctx.changed_files.as_deref().unwrap_or("").contains("b.txt"));
        assert_eq!(detect_project(&repo, &ctx).as_deref(), Some("my-repo"));
    }

    #[test]
    fn detached_head_yields_commit_without_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "test"]);
        std::fs::write(repo.join("a.txt"), "x").unwrap();
        git(&repo, &["add", "a.txt"]);
        git(&repo, &["commit", "-m", "init"]);
        git(&repo, &["checkout", "--detach"]);

        let ctx = GitContext::detect(&repo);
        assert!(ctx.commit.is_some());
        assert_eq!(ctx.branch, None);
    }

    #[test]
    fn empty_repo_yields_no_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);

        let ctx = GitContext::detect(&repo);
        assert!(ctx.repo_root.is_some());
        assert_eq!(ctx.commit, None);
    }
}
