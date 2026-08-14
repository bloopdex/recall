//! SQLite persistence layer (ADR-002/003/004/005/006).
//!
//! All SQL is parameterized; no user input is ever interpolated into SQL.
//! FTS synchronization is handled by triggers inside the schema, so every
//! write to `memories` is atomically reflected in the search index.

pub mod fts;
pub mod migrations;

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, Row};
use time::{format_description, macros::format_description, OffsetDateTime};

use crate::domain::memory::{normalize_for_comparison, Memory, MemoryEdits, NewMemory};
use crate::{Error, Result};

/// Timestamp storage format: RFC3339 UTC with millisecond precision, so
/// recency ordering is stable even for captures within the same second.
const CAPTURED_AT_FMT: &[format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");

/// A search result: a memory plus its FTS5 relevance rank.
/// Lower `rank` (bm25) = better match.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub memory: Memory,
    pub rank: f64,
}

/// Handle to the Recall database.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) the database at `path`, apply PRAGMAs and migrate
    /// to the latest schema version.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn, true)
    }

    /// In-memory database, used by unit tests.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn, false)
    }

    fn from_connection(mut conn: Connection, file_backed: bool) -> Result<Self> {
        // Safety PRAGMAs, documented in ADR-003.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        if file_backed {
            // WAL: readers never block the writer; crash-safe by design.
            conn.query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            })?;
            // NORMAL is safe with WAL for this single-process usage; FULL's
            // extra fsync cost buys nothing here.
            conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        }
        migrations::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Current schema version (0 when no migrations applied).
    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?)
    }

    /// Persist a memory. Runs in one transaction: the row insert and the
    /// FTS index update (via trigger) commit or roll back together, so
    /// partial writes cannot leave inconsistent records.
    pub fn insert_memory(
        &mut self,
        memory: &NewMemory,
        captured_at: OffsetDateTime,
    ) -> Result<i64> {
        let captured = captured_at
            .format(CAPTURED_AT_FMT)
            .map_err(|e| Error::Time(format!("cannot format capture timestamp: {e}")))?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO memories (
                 problem, solution, error, context, investigation, root_cause,
                 verification, environment, explanation, project, repo_path,
                 git_branch, git_commit, git_changed_files, cwd, captured_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                memory.problem,
                memory.solution,
                memory.error,
                memory.context,
                memory.investigation,
                memory.root_cause,
                memory.verification,
                memory.environment,
                memory.explanation,
                memory.project,
                memory.repo_path,
                memory.git_branch,
                memory.git_commit,
                memory.git_changed_files,
                memory.cwd,
                captured,
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// Fetch one memory by id.
    pub fn get_memory(&self, id: i64) -> Result<Option<Memory>> {
        self.conn
            .query_row(
                "SELECT id, problem, solution, error, context, investigation, root_cause,
                        verification, environment, explanation, project, repo_path,
                        git_branch, git_commit, git_changed_files, cwd, captured_at
                 FROM memories WHERE id = ?1",
                [id],
                memory_from_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(Error::Db(other)),
            })
    }

    /// Most recent memories, newest first.
    pub fn list_memories(&self, limit: usize) -> Result<Vec<Memory>> {
        self.query_memories(
            "SELECT id, problem, solution, error, context, investigation, root_cause,
                    verification, environment, explanation, project, repo_path,
                    git_branch, git_commit, git_changed_files, cwd, captured_at
             FROM memories ORDER BY captured_at DESC, id DESC LIMIT ?1",
            params![limit as i64],
        )
    }

    /// Update user-provided fields of an existing memory. Returns `false`
    /// when no memory with `id` exists. FTS5 stays synchronized via the
    /// schema's UPDATE trigger; the change is transactional.
    pub fn update_memory(&mut self, id: i64, edits: &MemoryEdits) -> Result<bool> {
        let mut parts: Vec<String> = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();

        // Column names are compile-time constants — only the values are
        // parameterized, so this dynamic SET clause is injection-safe.
        let set_field = |parts: &mut Vec<String>,
                         values: &mut Vec<rusqlite::types::Value>,
                         col: &str,
                         value: &Option<String>| {
            let n = values.len() + 1;
            parts.push(format!("{col} = ?{n}"));
            match value {
                Some(v) if !v.trim().is_empty() => {
                    values.push(rusqlite::types::Value::Text(v.trim().to_string()));
                }
                // Empty text means "clear the field".
                _ => values.push(rusqlite::types::Value::Null),
            }
        };
        for (col, value) in [
            ("problem", &edits.problem),
            ("solution", &edits.solution),
            ("error", &edits.error),
            ("context", &edits.context),
            ("investigation", &edits.investigation),
            ("root_cause", &edits.root_cause),
            ("verification", &edits.verification),
            ("environment", &edits.environment),
            ("explanation", &edits.explanation),
        ] {
            if value.is_some() {
                set_field(&mut parts, &mut values, col, value);
            }
        }
        debug_assert!(
            !parts.is_empty(),
            "update_memory requires at least one edit"
        );

        let id_placeholder = values.len() + 1;
        let sql = format!(
            "UPDATE memories SET {}, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?{id_placeholder}",
            parts.join(", ")
        );
        values.push(rusqlite::types::Value::Integer(id));

        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            &sql,
            rusqlite::params_from_iter(values.iter().map(|v| v as &dyn rusqlite::ToSql)),
        )?;
        tx.commit()?;
        Ok(changed > 0)
    }

    /// Find a near-identical memory (ADR-0011): same project, captured
    /// within `within_days`, and sharing either the normalized problem
    /// text or the normalized error text. Deterministic — no scoring.
    pub fn find_near_identical(
        &self,
        memory: &NewMemory,
        within_days: i64,
    ) -> Result<Option<Memory>> {
        let cutoff = (OffsetDateTime::now_utc() - time::Duration::days(within_days))
            .format(CAPTURED_AT_FMT)
            .map_err(|e| Error::Time(format!("cannot format cutoff timestamp: {e}")))?;

        let problem_norm = normalize_for_comparison(&memory.problem);
        let error_norm = memory.error.as_deref().map(normalize_for_comparison);

        // Candidate set: recent memories in the same project (NULL-safe).
        let candidates = match &memory.project {
            Some(project) => self.query_memories(
                "SELECT id, problem, solution, error, context, investigation, root_cause,
                        verification, environment, explanation, project, repo_path,
                        git_branch, git_commit, git_changed_files, cwd, captured_at
                 FROM memories
                 WHERE project = ?1 AND captured_at >= ?2
                 ORDER BY captured_at DESC, id DESC
                 LIMIT 20",
                params![project, cutoff],
            )?,
            None => self.query_memories(
                "SELECT id, problem, solution, error, context, investigation, root_cause,
                        verification, environment, explanation, project, repo_path,
                        git_branch, git_commit, git_changed_files, cwd, captured_at
                 FROM memories
                 WHERE project IS NULL AND captured_at >= ?1
                 ORDER BY captured_at DESC, id DESC
                 LIMIT 20",
                params![cutoff],
            )?,
        };

        Ok(candidates.into_iter().find(|candidate| {
            let same_problem = normalize_for_comparison(&candidate.problem) == problem_norm;
            let same_error = match &error_norm {
                Some(norm) => candidate
                    .error
                    .as_deref()
                    .is_some_and(|e| normalize_for_comparison(e) == *norm),
                None => false,
            };
            same_problem || same_error
        }))
    }

    /// Keyword search over the FTS5 index, ranked by weighted bm25
    /// (problem 5.0, solution 3.0, error 5.0, remaining fields 1.0 —
    /// see ADR-005), ties broken by recency.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let match_query = fts::build_match_query(query)?;
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.problem, m.solution, m.error, m.context, m.investigation,
                    m.root_cause, m.verification, m.environment, m.explanation,
                    m.project, m.repo_path, m.git_branch, m.git_commit,
                    m.git_changed_files, m.cwd, m.captured_at,
                    bm25(memories_fts, 5.0, 3.0, 5.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0) AS rank
             FROM memories_fts
             JOIN memories m ON m.id = memories_fts.rowid
             WHERE memories_fts MATCH ?1
             ORDER BY rank ASC, m.captured_at DESC, m.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![match_query, limit as i64], |row| {
            let memory = memory_from_row(row)?;
            let rank = row.get::<_, f64>(17)?;
            Ok(SearchHit { memory, rank })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::Db)
    }

    fn query_memories(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Memory>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, memory_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::Db)
    }
}

fn memory_from_row(row: &Row<'_>) -> rusqlite::Result<Memory> {
    let captured_at: String = row.get(16)?;
    // Stored as RFC3339 UTC ("...Z"); parse the naive datetime and assume
    // UTC — the literal "Z" is not an offset component for the parser.
    let captured_at = time::PrimitiveDateTime::parse(&captured_at, CAPTURED_AT_FMT)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Text, Box::new(e))
        })?
        .assume_utc();
    Ok(Memory {
        id: row.get(0)?,
        problem: row.get(1)?,
        solution: row.get(2)?,
        error: row.get(3)?,
        context: row.get(4)?,
        investigation: row.get(5)?,
        root_cause: row.get(6)?,
        verification: row.get(7)?,
        environment: row.get(8)?,
        explanation: row.get(9)?,
        project: row.get(10)?,
        repo_path: row.get(11)?,
        git_branch: row.get(12)?,
        git_commit: row.get(13)?,
        git_changed_files: row.get(14)?,
        cwd: row.get(15)?,
        captured_at,
    })
}
