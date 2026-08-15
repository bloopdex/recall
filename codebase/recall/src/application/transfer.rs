//! Export / import workflows (ADR-0024).
//!
//! Export produces the portable JSON format (`domain::export`): no
//! internal ids, schema-versioned, secrets redacted by default (opt-out
//! via `--include-secrets`). Import validates the format version, checks
//! every entry, detects duplicates by (project, normalized problem) —
//! skipping them unless `--force` — and preserves lifecycle status.
//! Embeddings are never exported: they are derived data tied to a
//! model/version and are rebuilt locally by `recall embeddings build`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use time::{format_description, macros::format_description, OffsetDateTime, PrimitiveDateTime};

use crate::domain::export::{ExportFile, ExportMemory, FORMAT_VERSION};
use crate::domain::memory::{normalize_for_comparison, MemoryStatus, NewMemory};
use crate::domain::sanitize;
use crate::infrastructure::database::Db;
use crate::{Error, Result};

const CAPTURED_AT_FMT: &[format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");

fn fmt_timestamp(t: OffsetDateTime) -> Result<String> {
    t.format(CAPTURED_AT_FMT)
        .map_err(|e| Error::Time(format!("cannot format timestamp: {e}")))
}

fn parse_timestamp(s: &str) -> Result<OffsetDateTime> {
    PrimitiveDateTime::parse(s, CAPTURED_AT_FMT)
        .map(|t| t.assume_utc())
        .map_err(|e| Error::InvalidInput(format!("invalid captured_at timestamp '{s}': {e}")))
}

/// Sanitize one optional text field unless raw export was requested.
fn field(text: &Option<String>, include_secrets: bool) -> Option<String> {
    let text = text.as_deref()?;
    if include_secrets {
        Some(text.to_string())
    } else {
        Some(sanitize::sanitize(text).sanitized)
    }
}

pub fn export(db: &Db, path: Option<&Path>, include_secrets: bool) -> Result<()> {
    let memories: Vec<ExportMemory> = db
        .memories_for_export()?
        .into_iter()
        .map(|m| -> Result<ExportMemory> {
            Ok(ExportMemory {
                problem: sanitize::sanitize(&m.problem).sanitized,
                solution: if include_secrets {
                    m.solution
                } else {
                    sanitize::sanitize(&m.solution).sanitized
                },
                error: field(&m.error, include_secrets),
                context: field(&m.context, include_secrets),
                investigation: field(&m.investigation, include_secrets),
                root_cause: field(&m.root_cause, include_secrets),
                verification: field(&m.verification, include_secrets),
                environment: field(&m.environment, include_secrets),
                explanation: field(&m.explanation, include_secrets),
                project: m.project,
                repo_path: m.repo_path,
                git_branch: m.git_branch,
                git_commit: m.git_commit,
                git_changed_files: m.git_changed_files,
                cwd: m.cwd,
                captured_at: fmt_timestamp(m.captured_at)?,
                status: m.status.as_str().to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let file = ExportFile {
        format_version: FORMAT_VERSION,
        exported_at: fmt_timestamp(OffsetDateTime::now_utc())?,
        recall_schema_version: db.schema_version()?,
        memories,
    };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| Error::Export(format!("cannot serialize export: {e}")))?;
    match path {
        Some(path) => {
            std::fs::write(path, format!("{json}\n")).map_err(Error::Io)?;
            println!(
                "{}Exported {} memories to {}",
                crate::ui::ok(),
                file.memories.len(),
                path.display()
            );
        }
        None => println!("{json}"),
    }
    Ok(())
}

pub fn import(db: &mut Db, path: &Path, force: bool) -> Result<()> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            Error::InvalidInput(format!(
                "not a valid Recall export: {} is not UTF-8 text",
                path.display()
            ))
        } else {
            Error::Io(std::io::Error::other(format!(
                "cannot read {}: {e}",
                path.display()
            )))
        }
    })?;
    let file: ExportFile = serde_json::from_str(&raw)
        .map_err(|e| Error::InvalidInput(format!("not a valid Recall export: {e}")))?;
    if file.format_version != FORMAT_VERSION {
        return Err(Error::InvalidInput(format!(
            "unsupported export format_version {} (this build reads version {FORMAT_VERSION})",
            file.format_version
        )));
    }
    // An export produced by a NEWER Recall may carry fields this build
    // does not know — serde would silently drop them. Refuse instead of
    // importing a lossy copy.
    let current_schema = db.schema_version()?;
    if file.recall_schema_version > current_schema {
        return Err(Error::InvalidInput(format!(
            "this export was produced by a newer Recall (schema v{}) than this \
             build (schema v{current_schema}); upgrade Recall before importing",
            file.recall_schema_version
        )));
    }

    // Validate every entry before inserting anything (all-or-nothing):
    // required fields, timestamps, and lifecycle status. A bad entry ANY
    // WHERE in the file must abort with zero rows inserted.
    for (i, entry) in file.memories.iter().enumerate() {
        if entry.problem.trim().is_empty() || entry.solution.trim().is_empty() {
            return Err(Error::InvalidInput(format!(
                "export entry #{} is missing a required field (problem/solution)",
                i + 1
            )));
        }
        if let Err(e) = parse_timestamp(&entry.captured_at) {
            return Err(Error::InvalidInput(format!("export entry #{}: {e}", i + 1)));
        }
        if entry.status != "active" && entry.status != "archived" {
            return Err(Error::InvalidInput(format!(
                "export entry #{} has an unknown lifecycle status {:?}",
                i + 1,
                entry.status
            )));
        }
    }

    // Duplicate detection: (project, normalized problem) already known.
    let mut known: HashMap<Option<String>, HashSet<String>> = HashMap::new();
    for memory in db.memories_for_export()? {
        known
            .entry(memory.project)
            .or_default()
            .insert(normalize_for_comparison(&memory.problem));
    }

    let mut imported = 0usize;
    let mut skipped = 0usize;
    for entry in &file.memories {
        let problem_norm = normalize_for_comparison(&entry.problem);
        if !force
            && known
                .get(&entry.project)
                .is_some_and(|problems| problems.contains(&problem_norm))
        {
            skipped += 1;
            continue;
        }
        let status = MemoryStatus::parse(&entry.status);
        let memory = NewMemory {
            problem: entry.problem.clone(),
            solution: entry.solution.clone(),
            error: entry.error.clone(),
            context: entry.context.clone(),
            investigation: entry.investigation.clone(),
            root_cause: entry.root_cause.clone(),
            verification: entry.verification.clone(),
            environment: entry.environment.clone(),
            explanation: entry.explanation.clone(),
            project: entry.project.clone(),
            repo_path: entry.repo_path.clone(),
            git_branch: entry.git_branch.clone(),
            git_commit: entry.git_commit.clone(),
            git_changed_files: entry.git_changed_files.clone(),
            cwd: entry.cwd.clone(),
        }
        .normalize();
        db.insert_memory_with_status(&memory, parse_timestamp(&entry.captured_at)?, status)?;
        known
            .entry(entry.project.clone())
            .or_default()
            .insert(problem_norm);
        imported += 1;
    }
    println!(
        "{}Imported {imported} memories, skipped {skipped} duplicate(s). Embeddings are rebuilt locally: run `recall embeddings build` to index the imported memories.",
        crate::ui::ok()
    );
    Ok(())
}
