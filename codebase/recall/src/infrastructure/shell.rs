//! Shell integration support (ADR-0017).
//!
//! Recall observes failed commands through each shell's **prompt hook** —
//! code the shell itself runs after every command — which records the last
//! command line and its exit code into environment variables that
//! `recall capture --from-shell` later reads. Recall never wraps, proxies,
//! or rewrites command execution: the user's commands behave exactly as
//! before, and the only shell-visible change is three exported variables.
//!
//! The snippet is installed as a **marked block** appended to the shell's
//! startup file (PowerShell profile or `~/.bashrc`/`~/.zshrc`). Markers make
//! install idempotent and uninstall exact: Recall only ever adds or removes
//! its own marked block and never touches anything outside it.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Environment variables carrying the failure snapshot from the shell
/// prompt hook to `recall capture --from-shell`. These are the ONLY
/// environment variables the shell integration reads or writes, and the
/// only ones `read_snapshot` ever touches (pinned by tests/security.rs).
pub const ENV_LAST_COMMAND: &str = "RECALL_LAST_COMMAND";
pub const ENV_LAST_EXIT_CODE: &str = "RECALL_LAST_EXIT_CODE";
pub const ENV_LAST_CWD: &str = "RECALL_LAST_CWD";

/// Marker lines delimiting Recall-owned blocks in startup files.
pub const MARKER_START: &str = "# >>> recall shell >>>";
pub const MARKER_END: &str = "# <<< recall shell <<<";

/// A shell flavor Recall can generate integration snippets for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    PowerShell,
    Bash,
    Zsh,
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shell::PowerShell => write!(f, "powershell"),
            Shell::Bash => write!(f, "bash"),
            Shell::Zsh => write!(f, "zsh"),
        }
    }
}

impl std::str::FromStr for Shell {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "powershell" | "pwsh" | "ps1" | "ps" => Ok(Shell::PowerShell),
            "bash" => Ok(Shell::Bash),
            "zsh" => Ok(Shell::Zsh),
            other => Err(Error::Shell(format!(
                "unknown shell '{other}' (supported: powershell, bash, zsh)"
            ))),
        }
    }
}

/// The failure snapshot the prompt hook recorded: what the user ran and
/// whether/how it failed. `command` is truncated by the reader (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSnapshot {
    pub command: String,
    /// Exit code of the failed command; `None` when unknown (e.g. a failed
    /// PowerShell cmdlet leaves `LASTEXITCODE` unset).
    pub exit_code: Option<i64>,
    pub cwd: Option<String>,
}

/// Read the failure snapshot left by the shell prompt hook.
/// Returns `None` when no hook context exists (vars unset/empty).
pub fn read_snapshot() -> Option<ShellSnapshot> {
    let command = std::env::var(ENV_LAST_COMMAND).ok()?;
    let command = crate::domain::sanitize::truncate_command(&command);
    if command.is_empty() {
        return None;
    }
    let exit_code = std::env::var(ENV_LAST_EXIT_CODE)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok());
    let cwd = std::env::var(ENV_LAST_CWD)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    Some(ShellSnapshot {
        command,
        exit_code,
        cwd,
    })
}

/// PowerShell profile snippet. Wraps an existing `prompt` function (saved as
/// `__recall_original_prompt`) instead of replacing it, records the snapshot
/// on every prompt render, and defines the `recall` dispatch function that
/// forwards known subcommands to the binary and treats anything else as a
/// search query (on-demand surfacing; never runs per command).
pub fn powershell_snippet() -> &'static str {
    r#"# >>> recall shell >>>
# Installed by `recall shell install`. Safe to remove: `recall shell uninstall`.
if (-not (Test-Path Function:\__recall_original_prompt)) {
    if (Test-Path Function:\prompt) {
        $function:__recall_original_prompt = ${function:prompt}
    } else {
        $function:__recall_original_prompt = { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
    }
}
function prompt {
    if ($?) {
        # Last command succeeded: no failure context to keep.
        $env:RECALL_LAST_COMMAND = ""
        $env:RECALL_LAST_EXIT_CODE = ""
        & $function:__recall_original_prompt
        return
    }
    $env:RECALL_LAST_EXIT_CODE = "$global:LASTEXITCODE"
    $env:RECALL_LAST_COMMAND = (Get-History -Count 1).CommandLine
    $env:RECALL_LAST_CWD = (Get-Location).Path
    & $function:__recall_original_prompt
}
function recall {
    $known = @('capture','search','list','edit','embeddings','shell','git','--help','--version','help')
    if ($args.Count -gt 0 -and $known -contains $args[0]) { recall.exe @args } else { recall.exe search @args }
}
# <<< recall shell <<<"#
}

/// Bash snippet: `PROMPT_COMMAND` chaining (prepended, so `$?` is read
/// before anything else runs), snapshot export, and the `recall` dispatch
/// function. Works in Git Bash on Windows as well as Linux/macOS bash.
pub fn bash_snippet() -> &'static str {
    r#"# >>> recall shell >>>
# Installed by `recall shell install`. Safe to remove: `recall shell uninstall`.
__recall_capture_last() {
    local __recall_status=$?
    export RECALL_LAST_EXIT_CODE="$__recall_status"
    export RECALL_LAST_COMMAND="$(history 1 | sed 's/^[[:space:]]*[0-9]*[[:space:]]*//')"
    export RECALL_LAST_CWD="$(pwd)"
}
if [[ -n "$PROMPT_COMMAND" ]]; then
    PROMPT_COMMAND="__recall_capture_last; $PROMPT_COMMAND"
else
    PROMPT_COMMAND="__recall_capture_last"
fi
recall() {
    case "${1:-}" in
        capture|search|list|edit|embeddings|shell|git|--help|--version|help)
            command recall "$@" ;;
        *) command recall search "$@" ;;
    esac
}
# <<< recall shell <<<"#
}

/// Zsh snippet: `precmd_functions` hook. Generated on the same mechanism as
/// bash but **not tested** in this environment (documented limitation).
pub fn zsh_snippet() -> &'static str {
    r#"# >>> recall shell >>>
# Installed by `recall shell install`. Safe to remove: `recall shell uninstall`.
__recall_capture_last() {
    export RECALL_LAST_EXIT_CODE="$?"
    export RECALL_LAST_COMMAND="${history[$((HISTCMD-1))]}"
    export RECALL_LAST_CWD="$PWD"
}
precmd_functions+=(__recall_capture_last)
recall() {
    case "${1:-}" in
        capture|search|list|edit|embeddings|shell|git|--help|--version|help)
            command recall "$@" ;;
        *) command recall search "$@" ;;
    esac
}
# <<< recall shell <<<"#
}

pub fn snippet_for(shell: Shell) -> &'static str {
    match shell {
        Shell::PowerShell => powershell_snippet(),
        Shell::Bash => bash_snippet(),
        Shell::Zsh => zsh_snippet(),
    }
}

/// Detect the shell the user is currently running under (for the default of
/// `recall shell init/install/uninstall/status`). `SHELL` decides for
/// bash/zsh; anything else on Windows defaults to PowerShell.
pub fn detect_shell() -> Shell {
    if let Ok(shell) = std::env::var("SHELL") {
        let shell = shell.to_lowercase();
        if shell.contains("zsh") {
            return Shell::Zsh;
        }
        if shell.contains("bash") {
            return Shell::Bash;
        }
    }
    Shell::PowerShell
}

/// Path of the startup file for a shell: the PowerShell current-user
/// profile, `~/.bashrc`, or `~/.zshrc`.
pub fn startup_file(shell: Shell) -> Result<PathBuf> {
    match shell {
        Shell::PowerShell => powershell_profile(),
        Shell::Bash => home_rc(".bashrc"),
        Shell::Zsh => home_rc(".zshrc"),
    }
}

/// Resolve the PowerShell current-user profile path by asking PowerShell
/// itself (`pwsh` preferred, `powershell` fallback) — the only reliable
/// source of `$PROFILE` without reimplementing its rules.
fn powershell_profile() -> Result<PathBuf> {
    for exe in ["pwsh", "powershell"] {
        let output = std::process::Command::new(exe)
            .args(["-NoProfile", "-Command", "Write-Output $PROFILE"])
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }
    }
    Err(Error::Shell(
        "PowerShell not found (tried `pwsh` and `powershell`); pass --shell explicitly or install manually with `recall shell init`".into(),
    ))
}

fn home_rc(file: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok())
        .ok_or_else(|| {
            Error::Shell(format!(
                "cannot determine the home directory for {file} (HOME/USERPROFILE unset)"
            ))
        })?;
    Ok(PathBuf::from(home).join(file))
}

/// What `recall shell status` reports about a startup file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Installed,
    NotInstalled,
    /// A start marker exists without its end marker: a broken block.
    Partial,
}

/// Does the file contain a complete recall block?
pub fn status_of(path: &Path) -> InstallStatus {
    let Ok(content) = std::fs::read_to_string(path) else {
        return InstallStatus::NotInstalled;
    };
    let has_start = content.contains(MARKER_START);
    let has_end = content.contains(MARKER_END);
    match (has_start, has_end) {
        (true, true) => InstallStatus::Installed,
        (true, false) => InstallStatus::Partial,
        (false, _) => InstallStatus::NotInstalled,
    }
}

/// Append the marked snippet block to `path`. Returns `true` when the block
/// was added, `false` when it was already present (idempotent).
pub fn install_into(path: &Path, snippet: &str) -> Result<bool> {
    if status_of(path) == InstallStatus::Installed {
        return Ok(false);
    }
    if status_of(path) == InstallStatus::Partial {
        return Err(Error::Shell(format!(
            "{} contains a broken recall block (start marker without end marker); remove it manually or run `recall shell uninstall`",
            path.display()
        )));
    }
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    if !content.is_empty() && !content.ends_with("\n\n") {
        content.push('\n');
    }
    content.push_str(snippet);
    content.push('\n');
    std::fs::write(path, content)?;
    Ok(true)
}

/// Remove the marked recall block from `path`. Returns `true` when a block
/// was removed; leaves everything outside the markers untouched.
pub fn uninstall_from(path: &Path) -> Result<bool> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };
    let (Some(start), Some(end)) = (content.find(MARKER_START), content.find(MARKER_END)) else {
        return Ok(false);
    };
    // Include the newline following the end marker.
    let end_of_block = end + MARKER_END.len();
    let mut end_with_newline = end_of_block;
    if content[end_with_newline..].starts_with('\n') {
        end_with_newline += 1;
    }
    let new_content = format!("{}{}", &content[..start], &content[end_with_newline..]);
    let trimmed = new_content.trim_end();
    if trimmed.is_empty() {
        std::fs::remove_file(path)?;
    } else {
        std::fs::write(path, format!("{trimmed}\n"))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_file(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("recall-shell-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn snippets_carry_both_markers() {
        for snippet in [powershell_snippet(), bash_snippet(), zsh_snippet()] {
            assert!(snippet.contains(MARKER_START));
            assert!(snippet.contains(MARKER_END));
        }
    }

    #[test]
    fn powershell_snippet_wraps_existing_prompt() {
        let snippet = powershell_snippet();
        assert!(snippet.contains("__recall_original_prompt"));
        assert!(snippet.contains("& $function:__recall_original_prompt"));
    }

    #[test]
    fn bash_snippet_chains_prompt_command() {
        let snippet = bash_snippet();
        assert!(snippet.contains("PROMPT_COMMAND=\"__recall_capture_last; $PROMPT_COMMAND\""));
    }

    #[test]
    fn install_is_idempotent_and_preserves_surrounding_content() {
        let path = tmp_file(
            "profile.ps1",
            "# my existing profile\nSet-Alias ll Get-ChildItem\n",
        );
        assert!(install_into(&path, powershell_snippet()).unwrap());
        assert_eq!(status_of(&path), InstallStatus::Installed);
        // Second install is a no-op.
        assert!(!install_into(&path, powershell_snippet()).unwrap());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# my existing profile"));
        assert!(content.contains("Set-Alias ll Get-ChildItem"));
        // Exactly one recall block.
        assert_eq!(content.matches(MARKER_START).count(), 1);
    }

    #[test]
    fn uninstall_removes_only_the_recall_block() {
        let path = tmp_file("bashrc.ps1", "export FOO=1\n");
        install_into(&path, bash_snippet()).unwrap();
        assert!(uninstall_from(&path).unwrap());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains(MARKER_START));
        assert!(content.contains("export FOO=1"));
        assert_eq!(status_of(&path), InstallStatus::NotInstalled);
    }

    #[test]
    fn uninstall_deletes_a_file_that_was_only_recall() {
        let path = tmp_file("only.ps1", "");
        install_into(&path, powershell_snippet()).unwrap();
        assert!(uninstall_from(&path).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn uninstall_without_block_is_a_no_op() {
        let path = tmp_file("plain.ps1", "echo hi\n");
        assert!(!uninstall_from(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "echo hi\n");
    }

    #[test]
    fn status_detects_partial_blocks() {
        let path = tmp_file("partial.ps1", "# >>> recall shell >>>\nbroken\n");
        assert_eq!(status_of(&path), InstallStatus::Partial);
    }

    /// One sequential test for the whole snapshot contract: the tests below
    /// mutate process-global env vars, so they must never run in parallel
    /// with each other (a cargo-test race that produced flakes).
    #[test]
    fn snapshot_reads_whitelisted_vars_with_limits() {
        // (1) Full snapshot.
        std::env::set_var(ENV_LAST_COMMAND, "cargo build --release");
        std::env::set_var(ENV_LAST_EXIT_CODE, "101");
        std::env::set_var(ENV_LAST_CWD, "C:\\tmp");
        let snap = read_snapshot().expect("snapshot present");
        assert_eq!(snap.command, "cargo build --release");
        assert_eq!(snap.exit_code, Some(101));
        assert_eq!(snap.cwd.as_deref(), Some("C:\\tmp"));

        // (2) No vars → no snapshot.
        std::env::remove_var(ENV_LAST_COMMAND);
        std::env::remove_var(ENV_LAST_EXIT_CODE);
        std::env::remove_var(ENV_LAST_CWD);
        assert!(read_snapshot().is_none());

        // (3) Long commands are truncated with a marker.
        std::env::set_var(ENV_LAST_COMMAND, "x".repeat(5000));
        let snap = read_snapshot().expect("snapshot present");
        assert!(snap.command.ends_with("... (truncated)"));
        assert!(snap.command.len() < 5000);

        // (4) Non-numeric exit codes degrade to None.
        std::env::set_var(ENV_LAST_COMMAND, "make");
        std::env::set_var(ENV_LAST_EXIT_CODE, "not-a-number");
        std::env::remove_var(ENV_LAST_CWD);
        let snap = read_snapshot().expect("snapshot present");
        assert_eq!(snap.exit_code, None);
        std::env::remove_var(ENV_LAST_COMMAND);
        std::env::remove_var(ENV_LAST_EXIT_CODE);
    }
}
