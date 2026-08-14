//! The capture workflow: gather the problem/solution (flags, stdin, or
//! interactive prompts), enrich with best-effort git/project context and a
//! UTC timestamp, validate, and persist atomically.

use std::io::{IsTerminal, Write};
use std::path::Path;

use time::OffsetDateTime;
use tracing::instrument;

use crate::cli::CaptureArgs;
use crate::domain::memory::NewMemory;
use crate::infrastructure::database::Db;
use crate::infrastructure::git::{detect_project, GitContext};
use crate::{Error, Result};

/// Outcome of a successful capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    pub id: i64,
    pub project: Option<String>,
}

#[instrument(skip(db, cwd))]
pub fn run(db: &mut Db, args: &CaptureArgs, cwd: &Path) -> Result<CaptureOutcome> {
    let problem = resolve_problem(args)?;
    let solution = resolve_solution(args)?;

    let git = GitContext::detect(cwd);
    let project = args.project.clone().or_else(|| detect_project(cwd, &git));

    let memory = NewMemory {
        problem,
        solution,
        error: args.error.clone(),
        context: args.context.clone(),
        investigation: args.investigation.clone(),
        root_cause: args.root_cause.clone(),
        verification: args.verification.clone(),
        environment: args.environment.clone(),
        explanation: args.explanation.clone(),
        project,
        repo_path: git.repo_root.map(|p| p.to_string_lossy().to_string()),
        git_branch: git.branch,
        git_commit: git.commit,
        git_changed_files: git.changed_files,
        cwd: Some(cwd.to_string_lossy().to_string()),
    }
    .normalize();
    memory.validate()?;

    let captured_at = OffsetDateTime::now_utc();
    let id = db.insert_memory(&memory, captured_at)?;

    tracing::info!(
        event = "capture.success",
        id,
        project = ?memory.project,
        git_commit = ?memory.git_commit,
        captures_count = 1,
    );

    Ok(CaptureOutcome {
        id,
        project: memory.project,
    })
}

/// Where the problem text comes from:
/// `--problem` flag > stdin (piped input or explicit `--stdin`) > prompt.
fn resolve_problem(args: &CaptureArgs) -> Result<String> {
    if let Some(problem) = &args.problem {
        return Ok(problem.clone());
    }
    if stdin_is_piped() || args.stdin {
        return read_stdin();
    }
    prompt("Problem")
}

/// Solution requires explicit input: `--solution` flag or a prompt.
/// (When the problem came from piped stdin, a prompt cannot be used —
/// stdin is exhausted.)
fn resolve_solution(args: &CaptureArgs) -> Result<String> {
    if let Some(solution) = &args.solution {
        return Ok(solution.clone());
    }
    if (stdin_is_piped() || args.stdin) && args.problem.is_none() {
        return Err(Error::InvalidInput(
            "stdin provides the problem; give the solution with --solution".into(),
        ));
    }
    prompt("Solution")
}

fn stdin_is_piped() -> bool {
    !std::io::stdin().is_terminal()
}

fn read_stdin() -> Result<String> {
    use std::io::Read;
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    Ok(text)
}

fn prompt(field: &str) -> Result<String> {
    eprint!("{field}: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line)
}
