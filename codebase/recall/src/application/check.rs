//! `recall check` — read-only consistency diagnostics (ADR-0028).
//!
//! A memory store is only trustworthy if the user can verify it. This
//! command runs SQLite's own structural check plus Recall's engine-level
//! invariants and prints every problem it finds. It NEVER repairs
//! anything: repair means restoring the pre-migration backup or
//! re-importing a Recall export (the documented recovery model).
//!
//! Checks:
//! - `integrity_check` — SQLite structural integrity (b-trees, pages)
//! - FTS5 `integrity-check` — the FTS index vs its content table
//! - row-count agreement: memories vs memories_fts (trigger sync)
//! - embedding orphans: embeddings rows whose memory no longer exists
//! - vec0 agreement: embeddings vs embeddings_vec rows (trigger sync)
//! - status validity: every row is `active` or `archived`

use rusqlite::Connection;

use crate::infrastructure::database::Db;
use crate::{Error, Result};

/// One problem found by a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckProblem {
    pub check: &'static str,
    pub detail: String,
}

/// The full diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub schema_version: i64,
    pub memories: i64,
    pub embeddings: i64,
    pub vec_enabled: bool,
    pub problems: Vec<CheckProblem>,
}

/// Run every check against the open database (read-only) and print the
/// report. Returns an error when problems were found, so the CLI exits
/// non-zero — scripts can gate on `recall check`.
pub fn run(db: &Db) -> Result<()> {
    let report = collect(db)?;

    println!("recall check");
    println!("  schema version: {}", report.schema_version);
    println!("  memories:       {}", report.memories);
    println!("  embeddings:     {}", report.embeddings);
    println!(
        "  vec0:           {}",
        if report.vec_enabled {
            "enabled"
        } else {
            "unavailable"
        }
    );

    if report.problems.is_empty() {
        println!("RESULT: OK — no consistency problems found.");
        return Ok(());
    }

    println!();
    for (i, problem) in report.problems.iter().enumerate() {
        println!("{}. {}: {}", i + 1, problem.check, problem.detail);
    }
    println!();
    println!(
        "RESULT: {} consistency problem(s) found.",
        report.problems.len()
    );
    println!(
        "Recall never repairs automatically. Recovery: restore the \
         <database>.pre-migration-backup snapshot, or re-import from a \
         Recall export (`recall import <file>`)."
    );
    Err(Error::CheckFailed(format!(
        "{} consistency problem(s) found (see the report above)",
        report.problems.len()
    )))
}

/// Gather all checks without printing.
pub fn collect(db: &Db) -> Result<CheckReport> {
    let mut problems = Vec::new();

    let (schema_version, memories, embeddings, vec_count) = db.with_connection(|c| {
        let q = |sql: &str| -> Result<i64> { Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?) };
        (
            db.schema_version(),
            q("SELECT count(*) FROM memories"),
            q("SELECT count(*) FROM embeddings"),
            q("SELECT count(*) FROM embeddings_vec"),
        )
    });
    let schema_version = schema_version?;
    let memories = memories?;
    let embeddings = embeddings?;
    let vec_count = vec_count?;

    db.with_connection(|c| {
        if let Some(detail) = integrity_check(c)? {
            problems.push(CheckProblem {
                check: "integrity_check",
                detail,
            });
        }
        if let Some(detail) = fts_integrity_check(c)? {
            problems.push(CheckProblem {
                check: "fts5_integrity_check",
                detail,
            });
        }
        // NOTE: no count-based FTS comparison. On an external-content FTS5
        // table, `count(*)` scans the content table, not the index, so
        // counts cannot detect an index desync — the `integrity-check`
        // command above is the authoritative check for that.
        let bad_status = count(
            c,
            "SELECT count(*) FROM memories WHERE status NOT IN ('active', 'archived')",
        )?;
        if bad_status > 0 {
            problems.push(CheckProblem {
                check: "status_validity",
                detail: format!("{bad_status} memories have an invalid lifecycle status"),
            });
        }
        let orphans = count(
            c,
            "SELECT count(*) FROM embeddings e
             WHERE NOT EXISTS (SELECT 1 FROM memories m WHERE m.id = e.memory_id)",
        )?;
        if orphans > 0 {
            problems.push(CheckProblem {
                check: "embedding_orphans",
                detail: format!("{orphans} embedding rows point at a missing memory"),
            });
        }
        Ok::<(), Error>(())
    })?;

    if db.vec_enabled() && vec_count != embeddings {
        problems.push(CheckProblem {
            check: "vec0_row_count",
            detail: format!(
                "embeddings={embeddings} but embeddings_vec={vec_count} (vector index out of sync)"
            ),
        });
    }

    Ok(CheckReport {
        schema_version,
        memories,
        embeddings,
        vec_enabled: db.vec_enabled(),
        problems,
    })
}

fn count(c: &Connection, sql: &str) -> Result<i64> {
    Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?)
}

/// `PRAGMA integrity_check` — "ok" or a human-readable report. Recall's
/// documented limitation: this verifies STRUCTURE, not cell payload
/// content (SQLite pages carry no checksums) — see ADR-0028.
fn integrity_check(c: &Connection) -> Result<Option<String>> {
    let report: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    Ok((report != "ok").then(|| report.lines().take(3).collect::<Vec<_>>().join(" | ")))
}

/// FTS5's own `integrity-check` command. It executes as a special INSERT
/// that RETURNS one row per problem and modifies nothing.
fn fts_integrity_check(c: &Connection) -> Result<Option<String>> {
    let mut stmt =
        c.prepare("INSERT INTO memories_fts(memories_fts, rank) VALUES('integrity-check', 1)")?;
    let problems = stmt.query([])?.mapped(|_| Ok(())).count();
    Ok((problems > 0).then(|| format!("the FTS5 index reported {problems} inconsistency row(s)")))
}
