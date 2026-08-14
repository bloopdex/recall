//! The capture workflow: gather the problem/solution (flags, stdin, or
//! interactive prompts), enrich with best-effort git/project context and a
//! UTC timestamp, check for near-identical memories (ADR-0011), validate,
//! and persist atomically.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use time::OffsetDateTime;
use tracing::instrument;

use crate::cli::CaptureArgs;
use crate::domain::memory::NewMemory;
use crate::infrastructure::database::Db;
use crate::infrastructure::git::{detect_project, GitContext};
use crate::{Error, Result};

/// Days a near-identical memory stays "recent enough" to block a capture.
/// Deterministic constant (ADR-0011); configurable later if ever needed.
const DEDUP_WINDOW_DAYS: i64 = 30;

/// Outcome of a successful capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutcome {
    pub id: i64,
    pub project: Option<String>,
    /// True when the capture was skipped because a near-identical memory
    /// already exists (see ADR-0011). `id` then refers to that memory.
    pub skipped: bool,
}

#[instrument(skip(db, cwd))]
pub fn run(db: &mut Db, args: &CaptureArgs, cwd: &Path) -> Result<CaptureOutcome> {
    run_with_io(
        db,
        args,
        cwd,
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )
}

/// The full capture flow with injected stdin/stdout-style streams, so the
/// input-resolution logic is unit-testable without a TTY.
pub fn run_with_io(
    db: &mut Db,
    args: &CaptureArgs,
    cwd: &Path,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<CaptureOutcome> {
    let stdin_is_piped = !std::io::stdin().is_terminal();
    let problem = resolve_problem(args, stdin_is_piped, input, prompt_out)?;
    let solution = resolve_solution(args, stdin_is_piped, input, prompt_out)?;

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

    // Deduplication: deterministic skip unless --force (ADR-0011).
    if !args.force {
        if let Some(existing) = db.find_near_identical(&memory, DEDUP_WINDOW_DAYS)? {
            tracing::info!(
                event = "capture.skipped_duplicate",
                existing_id = existing.id,
                new_project = ?memory.project,
                dedup_window_days = DEDUP_WINDOW_DAYS,
            );
            return Ok(CaptureOutcome {
                id: existing.id,
                project: memory.project,
                skipped: true,
            });
        }
    }

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
        skipped: false,
    })
}

/// Where the problem text comes from:
/// `--problem` flag > stdin (piped input or explicit `--stdin`) > prompt.
fn resolve_problem(
    args: &CaptureArgs,
    stdin_is_piped: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<String> {
    if let Some(problem) = &args.problem {
        return Ok(problem.clone());
    }
    if stdin_is_piped || args.stdin {
        return read_stdin(input);
    }
    prompt_line("Problem", input, prompt_out)
}

/// Solution requires explicit input: `--solution` flag or a prompt.
/// (When the problem came from piped stdin, a prompt cannot be used —
/// stdin is exhausted.)
fn resolve_solution(
    args: &CaptureArgs,
    stdin_is_piped: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<String> {
    if let Some(solution) = &args.solution {
        return Ok(solution.clone());
    }
    if (stdin_is_piped || args.stdin) && args.problem.is_none() {
        return Err(Error::InvalidInput(
            "stdin provides the problem; give the solution with --solution".into(),
        ));
    }
    prompt_line("Solution", input, prompt_out)
}

fn read_stdin(input: &mut dyn BufRead) -> Result<String> {
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    Ok(text)
}

/// Print `field: ` to the prompt stream and read one line from input.
fn prompt_line(field: &str, input: &mut dyn BufRead, prompt_out: &mut dyn Write) -> Result<String> {
    write!(prompt_out, "{field}: ")?;
    prompt_out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::CaptureArgs;

    fn args_with(problem: Option<&str>, solution: Option<&str>, stdin: bool) -> CaptureArgs {
        CaptureArgs {
            problem: problem.map(String::from),
            solution: solution.map(String::from),
            stdin,
            ..Default::default()
        }
    }

    fn lines(text: &str) -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(text.as_bytes().to_vec())
    }

    #[test]
    fn problem_flag_wins_over_stdin_and_prompt() {
        let args = args_with(Some("from flag"), None, false);
        let mut input = lines("from stdin");
        let mut out = Vec::new();
        let got = resolve_problem(&args, true, &mut input, &mut out).unwrap();
        assert_eq!(got, "from flag");
        assert!(
            out.is_empty(),
            "prompt must not be printed when the flag wins"
        );
    }

    #[test]
    fn piped_stdin_supplies_problem_and_never_prompts() {
        let args = args_with(None, None, false);
        let mut input = lines("piped problem\nmore lines\n");
        let mut out = Vec::new();
        let got = resolve_problem(&args, true, &mut input, &mut out).unwrap();
        assert_eq!(got, "piped problem\nmore lines\n");
        assert!(out.is_empty());
    }

    #[test]
    fn prompt_writes_prompt_and_reads_line() {
        let args = args_with(None, None, false);
        let mut input = lines("typed problem\n");
        let mut out = Vec::new();
        let got = resolve_problem(&args, false, &mut input, &mut out).unwrap();
        assert_eq!(got, "typed problem\n");
        assert_eq!(String::from_utf8(out).unwrap(), "Problem: ");
    }

    #[test]
    fn solution_requires_flag_when_stdin_supplied_the_problem() {
        let args = args_with(None, None, false);
        let mut input = lines("");
        let mut out = Vec::new();
        let err = resolve_solution(&args, true, &mut input, &mut out).unwrap_err();
        assert!(err.to_string().contains("--solution"));
    }

    #[test]
    fn solution_prompt_used_when_terminal() {
        let args = args_with(Some("problem via flag"), None, false);
        let mut input = lines("typed solution\n");
        let mut out = Vec::new();
        let got = resolve_solution(&args, false, &mut input, &mut out).unwrap();
        assert_eq!(got, "typed solution\n");
        assert_eq!(String::from_utf8(out).unwrap(), "Solution: ");
    }
}
