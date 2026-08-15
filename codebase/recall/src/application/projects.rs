//! The `recall projects` workflow (ADR-0022): a per-project overview of
//! the store — distinct project labels, memory counts, and last activity.
//! Derived entirely from the `memories` table; there is no project
//! registry to keep in sync (ADR-0021).

use crate::infrastructure::database::Db;
use crate::infrastructure::git::{detect_project, GitContext};
use crate::Result;

pub fn run(db: &Db, cwd: &std::path::Path) -> Result<()> {
    let stats = db.project_stats()?;
    if stats.is_empty() {
        println!("No memories yet — capture something first (`recall capture`).");
        return Ok(());
    }
    if crate::ui::pretty() {
        println!("{}{} project(s)", crate::ui::folder(), stats.len());
        println!();
    }

    let git = GitContext::detect(cwd);
    let current = detect_project(cwd, &git);

    let name_width = stats
        .iter()
        .map(|s| s.project.as_deref().unwrap_or("(no project)").len())
        .max()
        .unwrap_or(0)
        .max(4);
    println!(
        "{:<name_width$}  {:>6}  last capture",
        "project", "memories"
    );
    println!("{:-<name_width$}  ------  ------------", "");
    for stat in &stats {
        let name = stat.project.as_deref().unwrap_or("(no project)");
        let marker = if current.as_deref() == stat.project.as_deref() {
            "*"
        } else {
            " "
        };
        println!(
            "{marker}{name:<name_width$}  {:>6}  {}",
            stat.count, stat.last_captured
        );
    }
    println!();
    println!("* = the project detected from the current directory");
    Ok(())
}
