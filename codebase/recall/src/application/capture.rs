//! The capture workflow: gather the problem/solution (flags, stdin, or
//! interactive prompts), enrich with best-effort git/project context and a
//! UTC timestamp, check for near-identical memories (ADR-0011), validate,
//! and persist atomically.
//!
//! Two context modes come from the shell and git integrations
//! (ADR-0017/0019):
//!
//! - `--from-shell` reads the failure snapshot recorded by the shell
//!   prompt hook (last command + exit code) and pre-fills the problem.
//! - `--from-git` runs after a commit (post-commit hook) and pre-fills
//!   the problem from the commit subject, with the commit's changed files.
//!
//! A third mode covers CI failures (ADR-0030):
//!
//! - `--from-ci` runs inside an opt-in GitHub Actions failure step
//!   (`if: failure()`). The problem is built from the whitelisted
//!   GITHUB_* environment; piped stdin carries the log tail as the
//!   error. A `--solution` is REQUIRED — Recall stores fixes, not raw CI
//!   events.
//!
//! In all context modes the auto-captured text passes through the secret
//! sanitizer (ADR-0018): detected secrets are redacted, shown, and require
//! explicit confirmation before anything is persisted. In non-interactive
//! CI the gate fails closed (nothing stored). Problem + Solution remain
//! the required fields.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use time::OffsetDateTime;
use tracing::instrument;

use crate::cli::CaptureArgs;
use crate::domain::memory::NewMemory;
use crate::domain::sanitize;
use crate::infrastructure::database::Db;
use crate::infrastructure::git::{detect_project, CommitContext, GitContext};
use crate::infrastructure::shell;
use crate::{Error, Result};

/// Days a near-identical memory stays "recent enough" to block a capture.
/// Deterministic constant (ADR-0011); configurable later if ever needed.
const DEDUP_WINDOW_DAYS: i64 = 30;

/// Outcome of a capture run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// The memory was stored.
    Captured { id: i64, project: Option<String> },
    /// Skipped because a near-identical memory already exists (ADR-0011);
    /// `id` refers to that memory.
    SkippedDuplicate { id: i64, project: Option<String> },
    /// The user (or the environment) declined the capture; nothing was
    /// written. `reason` is a user-facing explanation.
    Declined { reason: String },
}

// `args` is skipped: CaptureArgs carries the raw problem/solution text and
// must never appear in logs (log-data policy, see the observability
// module doc).
#[instrument(skip(db, cwd, args))]
pub fn run(db: &mut Db, args: &CaptureArgs, cwd: &Path) -> Result<CaptureOutcome> {
    let stdin_is_tty = std::io::stdin().is_terminal();
    run_with_io(
        db,
        args,
        cwd,
        stdin_is_tty,
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )
}

/// The full capture flow with injected TTY flag and stdin/stdout-style
/// streams, so the input-resolution logic is unit-testable without a TTY.
pub fn run_with_io(
    db: &mut Db,
    args: &CaptureArgs,
    cwd: &Path,
    stdin_is_tty: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<CaptureOutcome> {
    let stdin_is_piped = !stdin_is_tty;
    let context_mode = args.from_shell || args.from_git || args.from_ci;
    // Interactive capture gets the friendly two-line prompt ("Problem"
    // then an arrow); everything else keeps the compact plain form.
    let pretty_prompts = stdin_is_tty && crate::ui::pretty();

    let git = GitContext::detect(cwd);
    let commit = if args.from_git {
        CommitContext::detect(cwd)
    } else {
        CommitContext::default()
    };

    // The post-commit hook runs in non-interactive contexts too (CI, GUI
    // git clients). There, with nothing explicit to go on, capture must
    // never block, prompt into the void, or fail the hook (ADR-0019).
    if args.from_git && !stdin_is_tty && args.problem.is_none() && args.solution.is_none() {
        return Ok(CaptureOutcome::Declined {
            reason: "Recall capture skipped: no interactive terminal. Run `recall capture --from-git` manually to capture this fix."
                .into(),
        });
    }

    // 1. Build the pre-fill from the shell snapshot or the commit context,
    //    sanitized (ADR-0018) before it is ever shown or stored.
    let prefill: Option<String> = if args.from_shell {
        let snap = shell::read_snapshot().ok_or_else(|| {
            Error::Shell(
                "no shell failure context found — install the integration with `recall shell install`, run the failing command, then try again"
                    .into(),
            )
        })?;
        let exit = snap
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".into());
        Some(sanitize::sanitize(&snap.command).sanitized)
            .map(|cmd| format!("Command failed (exit code {exit}): {cmd}"))
    } else if args.from_git {
        let project = args
            .project
            .clone()
            .or_else(|| detect_project(cwd, &git))
            .unwrap_or_else(|| "this repository".to_string());
        commit
            .subject
            .as_deref()
            .map(sanitize::sanitize)
            .map(|s| s.sanitized)
            .map(|subject| format!("Fix in {project}: {subject}"))
    } else if args.from_ci {
        // Deterministic problem text from workflow/job/event/step — the
        // run id is deliberately NOT part of it, so repeated failures of
        // the same job deduplicate (ADR-0011).
        let snap = crate::infrastructure::ci::read_snapshot()?;
        let step = args
            .step
            .as_deref()
            .map(|s| format!(" / step {s}"))
            .unwrap_or_default();
        Some(format!(
            "CI failure in {}{}{}{}",
            snap.workflow,
            if snap.job.is_empty() {
                String::new()
            } else {
                format!(" / {}", snap.job)
            },
            step,
            if snap.event.is_empty() {
                String::new()
            } else {
                format!(" ({})", snap.event)
            }
        ))
    } else {
        None
    };

    // 2. Error text: in context modes, piped stdin carries the failed
    //    command's output (not the problem, as in plain capture).
    let raw_error = if context_mode {
        args.error
            .clone()
            .or_else(|| {
                if stdin_is_piped {
                    read_stdin(input).ok()
                } else {
                    None
                }
            })
            .map(|t| sanitize::truncate_text(&t))
    } else {
        args.error.clone()
    };
    let error_report = raw_error.as_deref().map(sanitize::sanitize);

    // 3. Secret gate: if the auto-captured context contained secret-like
    //    patterns, show exactly what will be saved and require explicit
    //    confirmation before persisting (ADR-0018).
    if context_mode {
        let redactions = prefill
            .as_deref()
            .map(|p| sanitize::sanitize(p).redactions)
            .unwrap_or(0)
            + error_report.as_ref().map(|r| r.redactions).unwrap_or(0);
        if redactions > 0 {
            writeln!(
                prompt_out,
                "{}Warning: {redactions} secret-like pattern(s) detected in the captured context and redacted.",
                crate::ui::warn()
            )?;
            if let Some(p) = &prefill {
                writeln!(prompt_out, "  Problem will be: {p}")?;
            }
            if let Some(r) = &error_report {
                writeln!(
                    prompt_out,
                    "  Error will be:   {}",
                    first_line(&r.sanitized)
                )?;
            }
            if !confirm(prompt_out, input, "Save with redactions? [y/N]: ")? {
                return Ok(CaptureOutcome::Declined {
                    reason: "Not saved: redacted content was not confirmed.".into(),
                });
            }
        }
    }

    // 4. Problem: --problem flag wins; in context modes the (sanitized)
    //    pre-fill is offered at the prompt (Enter accepts, "skip" cancels);
    //    otherwise the interactive rules apply (piped stdin or plain
    //    prompt).
    let problem = match (&args.problem, &prefill) {
        (Some(p), _) => p.clone(),
        (None, Some(prefill)) => match resolve_prefilled("Problem", prefill, input, prompt_out)? {
            Some(p) => p,
            None => {
                return Ok(CaptureOutcome::Declined {
                    reason: "Capture cancelled.".into(),
                })
            }
        },
        (None, None) if stdin_is_piped || args.stdin => read_stdin(input)?,
        (None, None) => prompt_line("Problem", pretty_prompts, input, prompt_out)?,
    };

    // 5. Solution: --solution flag wins; piped stdin has already been
    //    consumed by the error text in context modes, so a prompt or the
    //    flag are the only options there.
    let solution = if let Some(solution) = &args.solution {
        solution.clone()
    } else if stdin_is_piped && !context_mode && args.problem.is_none() {
        return Err(Error::InvalidInput(
            "stdin provides the problem; give the solution with --solution".into(),
        ));
    } else if stdin_is_piped && context_mode {
        return Err(Error::InvalidInput(
            "stdin provides the error output; give the solution with --solution".into(),
        ));
    } else {
        prompt_line("Solution", pretty_prompts, input, prompt_out)?
    };

    let project = args
        .project
        .clone()
        .or_else(|| {
            // In CI the authoritative project identity is GITHUB_REPOSITORY's
            // repo name (ADR-0021/0030); fall back to local git detection.
            if args.from_ci {
                crate::infrastructure::ci::read_snapshot()
                    .ok()
                    .and_then(|s| s.repository)
                    .and_then(|r| {
                        crate::infrastructure::ci::repository_name(&r).map(|name| name.to_string())
                    })
            } else {
                None
            }
        })
        .or_else(|| detect_project(cwd, &git));

    // In CI, attach the run metadata to the context field (visible, and
    // deliberately NOT part of dedup — the deterministic problem text
    // carries deduplication).
    let context = if args.from_ci {
        args.context.clone().or_else(|| {
            let snap = crate::infrastructure::ci::read_snapshot().ok()?;
            let run = snap
                .run_id
                .clone()
                .map(|id| {
                    format!(
                        "run {}{}",
                        id,
                        snap.run_attempt
                            .as_deref()
                            .map(|a| format!(" (attempt {a})"))
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_default();
            let location = [snap.ref_name, snap.sha]
                .into_iter()
                .flatten()
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            Some(
                [run, location]
                    .into_iter()
                    .filter(|v| !v.is_empty())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    } else {
        args.context.clone()
    };

    let memory = NewMemory {
        problem,
        solution,
        error: error_report.as_ref().map(|r| r.sanitized.clone()),
        context,
        investigation: args.investigation.clone(),
        root_cause: args.root_cause.clone(),
        verification: args.verification.clone(),
        environment: args.environment.clone(),
        explanation: args.explanation.clone(),
        project,
        repo_path: git.repo_root.map(|p| p.to_string_lossy().to_string()),
        git_branch: git.branch,
        git_commit: git.commit,
        git_changed_files: if args.from_git {
            commit.changed_files.clone().or(git.changed_files)
        } else {
            git.changed_files
        },
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
            return Ok(CaptureOutcome::SkippedDuplicate {
                id: existing.id,
                project: memory.project,
            });
        }
    }

    let captured_at = OffsetDateTime::now_utc();
    let id = db.insert_memory(&memory, captured_at)?;

    // Semantic enrichment is best-effort and never blocks capture: if the
    // model or the vector store is unavailable, the memory is still saved
    // and remains keyword-searchable (ADR-0013/0014).
    enrich_embedding(db, id, &memory);

    tracing::info!(
        event = "capture.success",
        id,
        project = ?memory.project,
        git_commit = ?memory.git_commit,
        captures_count = 1,
    );

    Ok(CaptureOutcome::Captured {
        id,
        project: memory.project,
    })
}

/// Best-effort synchronous embedding of a freshly captured memory.
/// Failures degrade silently to "enrich later" (recall embeddings build).
fn enrich_embedding(db: &mut Db, id: i64, memory: &NewMemory) {
    use crate::infrastructure::embeddings::{embedded_text, Embedder, MODEL_ID, MODEL_VERSION};
    if !db.vec_enabled() {
        return;
    }
    let embedder = match Embedder::try_load() {
        Ok(e) => e,
        Err(_) => {
            tracing::info!(event = "embedding.unavailable", memory_id = id);
            return;
        }
    };
    let text = embedded_text(
        &memory.problem,
        memory.error.as_deref(),
        memory.context.as_deref(),
    );
    match embedder.embed_one(&text) {
        Ok(vector) => match db.insert_embedding(id, MODEL_ID, MODEL_VERSION, vector.len(), &vector)
        {
            Ok(()) => tracing::info!(event = "embedding.indexed", memory_id = id),
            Err(e) => tracing::warn!(event = "embedding.store_failed", memory_id = id, error = %e),
        },
        Err(e) => tracing::warn!(event = "embedding.failed", memory_id = id, error = %e),
    }
}

/// Prompt showing a pre-filled value: Enter accepts it, typing replaces
/// it, `skip` cancels (returns `None`).
fn resolve_prefilled(
    field: &str,
    prefill: &str,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<Option<String>> {
    write!(
        prompt_out,
        "{field} [{prefill}] (enter=accept, 'skip'=cancel): "
    )?;
    prompt_out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    let line = line.trim_end_matches(['\r', '\n']).to_string();
    if line.trim().eq_ignore_ascii_case("skip") {
        return Ok(None);
    }
    if line.trim().is_empty() {
        return Ok(Some(prefill.to_string()));
    }
    Ok(Some(line))
}

/// Ask a yes/no question; anything other than an explicit yes is a no.
fn confirm(prompt_out: &mut dyn Write, input: &mut dyn BufRead, question: &str) -> Result<bool> {
    write!(prompt_out, "{question}")?;
    prompt_out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// Where the problem text comes from in plain (non-context) capture:
/// `--problem` flag > stdin (piped input or explicit `--stdin`) > prompt.
///
/// `pretty` switches the prompt to the friendly two-line form
/// ("Problem" on its own line, answer after an arrow) used only for
/// interactive terminals; piped prompts keep the compact `Field: ` form.
fn prompt_line(
    field: &str,
    pretty: bool,
    input: &mut dyn BufRead,
    prompt_out: &mut dyn Write,
) -> Result<String> {
    if pretty {
        writeln!(prompt_out, "{field}")?;
        write!(prompt_out, "{} ", crate::ui::arrow())?;
    } else {
        write!(prompt_out, "{field}: ")?;
    }
    prompt_out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(line)
}

fn read_stdin(input: &mut dyn BufRead) -> Result<String> {
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    Ok(text)
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sanitize::SanitizeReport;

    fn lines(text: &str) -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(text.as_bytes().to_vec())
    }

    #[test]
    fn prefill_accepts_on_empty_line() {
        let mut input = lines("\n");
        let mut out = Vec::new();
        let got = resolve_prefilled("Problem", "Command failed", &mut input, &mut out).unwrap();
        assert_eq!(got.as_deref(), Some("Command failed"));
        assert!(String::from_utf8(out).unwrap().contains("enter=accept"));
    }

    #[test]
    fn prefill_accepts_typed_override() {
        let mut input = lines("a better problem\n");
        let mut out = Vec::new();
        let got = resolve_prefilled("Problem", "prefill", &mut input, &mut out).unwrap();
        assert_eq!(got.as_deref(), Some("a better problem"));
    }

    #[test]
    fn prefill_skip_cancels() {
        let mut input = lines("skip\n");
        let mut out = Vec::new();
        let got = resolve_prefilled("Problem", "prefill", &mut input, &mut out).unwrap();
        assert_eq!(got, None);
    }

    #[test]
    fn confirm_accepts_explicit_yes_only() {
        for (answer, expected) in [
            ("y", true),
            ("yes", true),
            ("Y", true),
            ("n", false),
            ("", false),
            ("maybe", false),
        ] {
            let mut input = lines(&format!("{answer}\n"));
            let mut out = Vec::new();
            assert_eq!(
                confirm(&mut out, &mut input, "Save? [y/N]: ").unwrap(),
                expected
            );
        }
    }

    #[test]
    fn prompt_writes_prompt_and_reads_line() {
        let mut input = lines("typed problem\n");
        let mut out = Vec::new();
        let got = prompt_line("Problem", false, &mut input, &mut out).unwrap();
        assert_eq!(got, "typed problem\n");
        assert_eq!(String::from_utf8(out).unwrap(), "Problem: ");
    }

    #[test]
    fn pretty_prompt_uses_the_two_line_form() {
        let mut input = lines("typed problem\n");
        let mut out = Vec::new();
        let got = prompt_line("Problem", true, &mut input, &mut out).unwrap();
        assert_eq!(got, "typed problem\n");
        let out = String::from_utf8(out).unwrap();
        // The shape is pinned: the field on its own line, the answer
        // line after an arrow. The arrow glyph itself is environment-
        // dependent (fancy terminal vs plain output).
        assert!(out.starts_with("Problem\n"), "{out:?}");
        assert_eq!(out.lines().count(), 2, "two lines: {out:?}");
        assert!(
            out.lines().nth(1).unwrap().ends_with(' '),
            "answer line waits after the arrow: {out:?}"
        );
    }

    #[test]
    fn secret_gate_count_combines_prefill_and_error() {
        // Mirrors the arithmetic used in run_with_io for the gate.
        let prefill = Some(SanitizeReport {
            sanitized: "x".into(),
            redactions: 1,
        });
        let error = Some(SanitizeReport {
            sanitized: "y".into(),
            redactions: 2,
        });
        let total = prefill.map(|p| p.redactions).unwrap_or(0)
            + error.as_ref().map(|r| r.redactions).unwrap_or(0);
        assert_eq!(total, 3);
    }
}
