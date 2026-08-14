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

use crate::domain::memory::{Memory, NewMemory};
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
