//! The `recall git` workflow: install / uninstall / status of the
//! post-commit hook (ADR-0019/0020). Installation is always explicit,
//! reversible, and never overwrites a user's hook.

use crate::cli::GitCommand;
use crate::infrastructure::git;
use crate::Result;

/// The single hook Recall manages.
pub const HOOK_NAME: &str = "post-commit";

pub fn run(command: &GitCommand) -> Result<()> {
    let cwd = std::env::current_dir()?;
    match command {
        GitCommand::Install { append } => install(&cwd, *append),
        GitCommand::Uninstall => uninstall(&cwd),
        GitCommand::Status => status(&cwd),
    }
}

fn install(cwd: &std::path::Path, append: bool) -> Result<()> {
    match git::install_hook(cwd, HOOK_NAME, append)? {
        git::InstallOutcome::Installed(path) => {
            println!(
                "{}Installed: recall post-commit hook written to {} — after each commit, recall offers to capture the fix. Remove with `recall git uninstall`.",
                crate::ui::ok(),
                path.display()
            );
            if crate::ui::pretty() {
                println!(
                    "{}Make a fix commit — the hook will offer to capture it",
                    crate::ui::arrow()
                );
            }
        }
        git::InstallOutcome::Appended(path) => println!(
            "{}Appended: recall block added to your existing hook at {} (your content is preserved).",
            crate::ui::ok(),
            path.display()
        ),
        git::InstallOutcome::AlreadyInstalled(path) => println!(
            "Already installed: recall block present in {}.",
            path.display()
        ),
    }
    Ok(())
}

fn uninstall(cwd: &std::path::Path) -> Result<()> {
    match git::uninstall_hook(cwd, HOOK_NAME)? {
        true => println!(
            "{}Uninstalled: recall hook removed (any user hook content was preserved).",
            crate::ui::ok()
        ),
        false => println!("Not installed: no recall hook found in this repository."),
    }
    Ok(())
}

fn status(cwd: &std::path::Path) -> Result<()> {
    let state = match git::hook_status(cwd, HOOK_NAME) {
        git::HookStatus::Installed => "installed",
        git::HookStatus::NotInstalled => "not installed",
        git::HookStatus::NotApplicable => "not applicable (not a working-tree git repository)",
    };
    println!("post-commit hook: {state}");
    Ok(())
}
