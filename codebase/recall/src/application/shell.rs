//! The `recall shell` workflow: init / install / uninstall / status of the
//! shell integration snippet (ADR-0017). All edits are confined to the
//! marked recall block in the shell's startup file.

use crate::cli::ShellCommand;
use crate::infrastructure::shell::{self, Shell};
use crate::Result;

pub fn run(command: &ShellCommand) -> Result<()> {
    match command {
        ShellCommand::Init { shell } => init(shell.unwrap_or_else(shell::detect_shell)),
        ShellCommand::Install { shell } => install(shell.unwrap_or_else(shell::detect_shell)),
        ShellCommand::Uninstall { shell } => uninstall(shell.unwrap_or_else(shell::detect_shell)),
        ShellCommand::Status { shell } => status(shell.unwrap_or_else(shell::detect_shell)),
    }
}

fn init(shell: Shell) -> Result<()> {
    let snippet = shell::snippet_for(shell);
    println!("# {shell} integration snippet — append to your startup file, or run:");
    println!("#   recall shell install");
    println!();
    println!("{snippet}");
    Ok(())
}

fn install(shell: Shell) -> Result<()> {
    let path = shell::startup_file(shell)?;
    match shell::install_into(&path, shell::snippet_for(shell))? {
        true => {
            println!(
                "{}Installed: recall shell integration added to {} (start a new {shell} session to activate).",
                crate::ui::ok(),
                path.display()
            );
            if crate::ui::pretty() {
                println!(
                    "{}After your next failed command: recall capture --from-shell",
                    crate::ui::arrow()
                );
            }
        }
        false => println!(
            "Already installed: the recall block is present in {}.",
            path.display()
        ),
    }
    Ok(())
}

fn uninstall(shell: Shell) -> Result<()> {
    let path = shell::startup_file(shell)?;
    match shell::uninstall_from(&path)? {
        true => println!(
            "{}Uninstalled: recall shell integration removed from {} (existing content preserved).",
            crate::ui::ok(),
            path.display()
        ),
        false => println!(
            "Not installed: no recall block found in {}.",
            path.display()
        ),
    }
    Ok(())
}

fn status(shell: Shell) -> Result<()> {
    let path = shell::startup_file(shell)?;
    let state = match shell::status_of(&path) {
        shell::InstallStatus::Installed => "installed",
        shell::InstallStatus::NotInstalled => "not installed",
        shell::InstallStatus::Partial => "PARTIAL (start marker without end marker — broken block)",
    };
    println!("{shell} integration: {state} ({})", path.display());
    Ok(())
}
