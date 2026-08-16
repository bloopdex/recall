//! Best-effort git metadata capture.
//!
//! Strategy (ADR-008): spawn the `git` executable with fixed argument
//! vectors — never a shell, never user-provided strings — and treat every
//! failure (missing binary, not a repo, detached HEAD, empty repo) as
//! "metadata unavailable". Git metadata is never a requirement for capture.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Error, Result};

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

// ---------------------------------------------------------------------------
// Commit context (ADR-0019): what a just-created commit contains.
// ---------------------------------------------------------------------------

/// Context about the commit at `HEAD`, for `recall capture --from-git`
/// (ADR-0019). Best-effort like `GitContext`: every field degrades to
/// `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitContext {
    /// First line of the commit message (`git log -1 --format=%s`).
    pub subject: Option<String>,
    /// Files changed by that commit (`git show --name-only HEAD`), capped
    /// at 50 lines like the working-tree changed-files list (ADR-0008).
    pub changed_files: Option<String>,
}

impl CommitContext {
    pub fn detect(cwd: &Path) -> Self {
        let mut ctx = CommitContext::default();
        if let Some(subject) = git_output(cwd, &["log", "-1", "--format=%s"]) {
            let subject = subject.trim().to_string();
            if !subject.is_empty() {
                ctx.subject = Some(subject);
            }
        }
        if let Some(files) = git_output(cwd, &["show", "--format=", "--name-only", "HEAD"]) {
            let files: Vec<&str> = files.lines().filter(|l| !l.trim().is_empty()).collect();
            let capped: Vec<&str> = files.into_iter().take(50).collect();
            if !capped.is_empty() {
                ctx.changed_files = Some(capped.join("\n"));
            }
        }
        ctx
    }
}

// ---------------------------------------------------------------------------
// Hook lifecycle (ADR-0019): install / uninstall / status with preservation
// of user hooks (ADR-0020).
// ---------------------------------------------------------------------------

/// Marker lines delimiting the Recall-owned block inside a hook script.
pub const HOOK_MARKER_START: &str = "# >>> recall git hook >>>";
pub const HOOK_MARKER_END: &str = "# <<< recall git hook <<<";

/// The recall block appended to an existing user hook by `--append`.
fn hook_block() -> String {
    format!(
        "{HOOK_MARKER_START}\n# Installed by `recall git install`. Non-blocking: any Recall failure\n# leaves the commit untouched. Remove with `recall git uninstall`.\nif command -v recall >/dev/null 2>&1; then\n  recall capture --from-git || true\nfi\n{HOOK_MARKER_END}"
    )
}

/// The full hook script written when no user hook exists.
pub fn hook_script() -> String {
    format!("#!/bin/sh\n{}", hook_block())
}

/// Resolve the repository's hooks directory via `git rev-parse --git-path
/// hooks` — correct for worktrees and separated git dirs, unlike assuming
/// `.git/hooks`.
pub fn hooks_dir(cwd: &Path) -> Result<PathBuf> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--git-path", "hooks"])
        .output()
        .map_err(|e| Error::Git(format!("git not available: {e}")))?;
    if !out.status.success() {
        return Err(Error::Git(
            "not a git repository (git rev-parse --git-path hooks failed)".into(),
        ));
    }
    let rel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(PathBuf::from(rel))
}

fn is_bare_repo(cwd: &Path) -> bool {
    git_output(cwd, &["rev-parse", "--is-bare-repository"])
        .map(|v| v.trim() == "true")
        .unwrap_or(false)
}

/// Outcome of `recall git install`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Recall hook written (no existing user hook).
    Installed(PathBuf),
    /// Recall block appended to the existing user hook (--append).
    Appended(PathBuf),
    /// Recall block already present.
    AlreadyInstalled(PathBuf),
}

/// Install the `post-commit` hook. Never overwrites a user hook: if one
/// exists without a recall block, installation is refused unless
/// `append` is set, in which case the recall block is appended after the
/// user's content.
pub fn install_hook(cwd: &Path, name: &str, append: bool) -> Result<InstallOutcome> {
    if is_bare_repo(cwd) {
        return Err(Error::Git(
            "bare repository has no working tree; git hooks do not apply".into(),
        ));
    }
    let dir = hooks_dir(cwd)?;
    let path = dir.join(name);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing.contains(HOOK_MARKER_START) && existing.contains(HOOK_MARKER_END) {
            return Ok(InstallOutcome::AlreadyInstalled(path));
        }
        if !append {
            return Err(Error::Git(format!(
                "existing {name} hook found at {} — not touching it. Run `recall git install --append` to add a non-blocking recall call to it, or `recall git uninstall` if it is a recall hook.",
                path.display()
            )));
        }
        let mut content = existing;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(&hook_block());
        content.push('\n');
        std::fs::write(&path, content)?;
        return Ok(InstallOutcome::Appended(path));
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, hook_script())?;
    Ok(InstallOutcome::Installed(path))
}

/// Remove the recall block from the hook. If nothing but the recall block
/// remains (recall-installed hook), the file is deleted; user content is
/// always preserved.
pub fn uninstall_hook(cwd: &Path, name: &str) -> Result<bool> {
    let dir = match hooks_dir(cwd) {
        Ok(d) => d,
        Err(_) => return Ok(false),
    };
    let path = dir.join(name);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Ok(false);
    };
    let (Some(start), Some(end)) = (
        content.find(HOOK_MARKER_START),
        content.find(HOOK_MARKER_END),
    ) else {
        return Ok(false);
    };
    let end_of_block = end + HOOK_MARKER_END.len();
    let mut end_with_newline = end_of_block;
    if content[end_with_newline..].starts_with('\n') {
        end_with_newline += 1;
    }
    let new_content = format!("{}{}", &content[..start], &content[end_with_newline..]);
    let trimmed = new_content.trim_end();
    // A hook recall installed itself consists of a shebang plus the block:
    // removing the block leaves only the shebang, which is worth nothing.
    let without_shebang = trimmed
        .strip_prefix("#!/bin/sh")
        .map(str::trim)
        .unwrap_or(trimmed);
    if without_shebang.is_empty() {
        std::fs::remove_file(&path)?;
    } else {
        std::fs::write(&path, format!("{trimmed}\n"))?;
    }
    Ok(true)
}

/// Hook state as seen by `recall git status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStatus {
    Installed,
    NotInstalled,
    /// Recall block present but the repository has no working tree
    /// (bare) or is not a git repository at all.
    NotApplicable,
}

pub fn hook_status(cwd: &Path, name: &str) -> HookStatus {
    if is_bare_repo(cwd) {
        return HookStatus::NotApplicable;
    }
    let Ok(dir) = hooks_dir(cwd) else {
        return HookStatus::NotApplicable;
    };
    let path = dir.join(name);
    let Ok(content) = std::fs::read_to_string(path) else {
        return HookStatus::NotInstalled;
    };
    if content.contains(HOOK_MARKER_START) && content.contains(HOOK_MARKER_END) {
        HookStatus::Installed
    } else {
        HookStatus::NotInstalled
    }
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
        // `git rev-parse --show-toplevel` may spell the root differently
        // than the path the repo was created with (Windows: short 8.3
        // names and slash direction; macOS: /tmp symlinks). Compare the
        // canonicalized locations, not the raw strings — the assertion
        // still pins that the discovered root IS this repository.
        let reported = ctx
            .repo_root
            .as_deref()
            .expect("the repo root must be discovered");
        assert_eq!(
            std::fs::canonicalize(reported).expect("the reported root must exist"),
            std::fs::canonicalize(&repo).expect("the temp repo must exist"),
        );
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
