//! SQLite persistence layer (ADR-002/003/004/005/006).
//!
//! All SQL is parameterized; no user input is ever interpolated into SQL.
//! FTS synchronization is handled by triggers inside the schema, so every
//! write to `memories` is atomically reflected in the search index.

pub mod fts;
pub mod migrations;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, Row};
use time::{format_description, macros::format_description, OffsetDateTime};

use crate::domain::memory::{normalize_for_comparison, Memory, MemoryEdits, NewMemory};
use crate::infrastructure::embeddings::{EMBED_DIMS, MODEL_ID, MODEL_VERSION};
use crate::{Error, Result};

/// Reciprocal-rank-fusion constant: softens rank differences between the
/// two engines (ADR-0016).
const RRF_K: f64 = 60.0;
/// Weight of the keyword engine's rank contribution.
const FTS_WEIGHT: f64 = 1.0;
/// Weight of the semantic engine's rank contribution.
const SEM_WEIGHT: f64 = 0.9;
/// Candidate pool per engine before fusion.
const CANDIDATE_POOL: usize = 50;

/// Register the sqlite-vec extension so every new connection gets `vec0`
/// (sqlite-vec 0.1.9 loads via `sqlite3_auto_extension`).
fn register_vec_extension() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        // sqlite-vec declares `sqlite3_vec_init` without the extension-callback
        // signature, so a transmute into the signature sqlite3_auto_extension
        // expects is required (same approach as sqlite-vec's own test suite).
        type InitFn = unsafe extern "C" fn() -> ();
        type Callback = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        let init: InitFn = sqlite_vec::sqlite3_vec_init;
        let callback: Callback = std::mem::transmute::<InitFn, Callback>(init);
        rusqlite::ffi::sqlite3_auto_extension(Some(callback));
    });
}

/// A hybrid search result: the memory plus the per-engine signals that
/// produced its fused rank (ADR-0016 — every score is explainable).
#[derive(Debug, Clone, PartialEq)]
pub struct HybridHit {
    pub memory: Memory,
    /// Keyword engine rank contribution (RRF of the FTS position).
    pub fts_rank: Option<f64>,
    /// Semantic engine: cosine similarity in [0, 1].
    pub sem_similarity: Option<f64>,
    /// Combined score; higher is better.
    pub fused_score: f64,
}

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
    /// Whether the sqlite-vec extension loaded and `embeddings_vec` exists.
    vec_enabled: bool,
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
        // MUST run before the connection is created: rusqlite applies
        // auto-registered extensions only to new connections.
        register_vec_extension();
        let conn = Connection::open(path)?;
        Self::from_connection(conn, true)
    }

    /// In-memory database, used by unit tests.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        register_vec_extension();
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

        // Semantic layer: enrich when the extension loads, degrade to
        // FTS-only when it doesn't (ADR-0014).
        let vec_enabled = Self::setup_vec(&mut conn).unwrap_or(false);
        Ok(Self { conn, vec_enabled })
    }

    /// Create the vec0 index table + sync triggers. Returns false when the
    /// sqlite-vec extension is unavailable — semantic search then stays
    /// off while keyword search keeps working.
    fn setup_vec(conn: &mut Connection) -> Result<bool> {
        register_vec_extension();
        let version: rusqlite::Result<String> =
            conn.query_row("SELECT vec_version()", [], |r| r.get(0));
        if version.is_err() {
            tracing::warn!(
                event = "vec.unavailable",
                reason = "sqlite-vec extension failed to load"
            );
            return Ok(false);
        }
        let ddl = format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings_vec USING vec0(
                 embedding float[{EMBED_DIMS}] distance_metric=cosine
             );"
        );
        conn.execute_batch(&ddl)?;
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS trg_emb_ai AFTER INSERT ON embeddings BEGIN
                 INSERT INTO embeddings_vec(rowid, embedding) VALUES (new.memory_id, new.vector);
             END;
             CREATE TRIGGER IF NOT EXISTS trg_emb_ad AFTER DELETE ON embeddings BEGIN
                 DELETE FROM embeddings_vec WHERE rowid = old.memory_id;
             END;
             CREATE TRIGGER IF NOT EXISTS trg_emb_au AFTER UPDATE ON embeddings BEGIN
                 DELETE FROM embeddings_vec WHERE rowid = old.memory_id;
                 INSERT INTO embeddings_vec(rowid, embedding) VALUES (new.memory_id, new.vector);
             END;",
        )?;
        Ok(true)
    }

    /// Whether the semantic layer is available on this connection.
    pub fn vec_enabled(&self) -> bool {
        self.vec_enabled
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

    /// Delete a memory and — via FK cascade and the FTS/vec triggers —
    /// its keyword and vector index entries. Returns `false` when no
    /// memory with `id` exists. (Used by tests; retention policies arrive
    /// in Phase 5.)
    pub fn delete_memory(&mut self, id: i64) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute("DELETE FROM memories WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(changed > 0)
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

    /// Store (or replace) the embedding for a memory. Runs in one
    /// transaction; the vec0 index follows via triggers. Only the current
    /// model/version should ever be written here.
    pub fn insert_embedding(
        &mut self,
        memory_id: i64,
        model: &str,
        model_version: &str,
        dims: usize,
        vector: &[f32],
    ) -> Result<()> {
        if !self.vec_enabled {
            return Err(Error::Embedding(
                "sqlite-vec is unavailable in this build/connection".into(),
            ));
        }
        if vector.len() != EMBED_DIMS {
            return Err(Error::Embedding(format!(
                "vector has {} dimensions, expected {EMBED_DIMS}",
                vector.len()
            )));
        }
        if vector.iter().any(|x| !x.is_finite()) {
            return Err(Error::Embedding(
                "vector contains NaN or infinite values — refusing to store a degenerate vector"
                    .into(),
            ));
        }
        let blob = to_blob(vector);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO embeddings (memory_id, model, model_version, dims, vector)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(memory_id) DO UPDATE SET
                 model = excluded.model,
                 model_version = excluded.model_version,
                 dims = excluded.dims,
                 vector = excluded.vector,
                 created_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![memory_id, model, model_version, dims as i64, blob],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Remove a memory's embedding (metadata row + vec0 index entry).
    pub fn delete_embedding(&mut self, memory_id: i64) -> Result<()> {
        if !self.vec_enabled {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM embeddings WHERE memory_id = ?1", [memory_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Memory ids whose embedding is missing or was produced by another
    /// model/version (stale).
    pub fn embedding_backlog(&self, model: &str, model_version: &str) -> Result<Vec<i64>> {
        if !self.vec_enabled {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT m.id FROM memories m
             LEFT JOIN embeddings e ON e.memory_id = m.id
             WHERE e.memory_id IS NULL OR e.model != ?1 OR e.model_version != ?2
             ORDER BY m.id",
        )?;
        let rows = stmt.query_map(params![model, model_version], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::Db)
    }

    /// (total memories, memories with a current embedding, stale/missing count).
    pub fn embedding_stats(&self, model: &str, model_version: &str) -> Result<(i64, i64, i64)> {
        if !self.vec_enabled {
            return Ok((0, 0, 0));
        }
        let total: i64 = self
            .conn
            .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))?;
        let current: i64 = self.conn.query_row(
            "SELECT count(*) FROM embeddings WHERE model = ?1 AND model_version = ?2",
            params![model, model_version],
            |r| r.get(0),
        )?;
        Ok((total, current, total - current))
    }

    /// k-nearest semantic search over the vec0 index: (memory_id, cosine
    /// distance). Only embeddings of the given model/version participate.
    pub fn semantic_search(
        &self,
        query_vector: &[f32],
        k: usize,
        model: &str,
        model_version: &str,
    ) -> Result<Vec<(i64, f64)>> {
        if !self.vec_enabled {
            return Ok(Vec::new());
        }
        let blob = to_blob(query_vector);
        // The MATCH must drive the vec0 scan: joining the metadata table in
        // the same statement lets SQLite reorder the plan on larger tables
        // and emit NULL `distance` (regression-pinned by tests/semantic_10k.rs).
        let mut stmt = self.conn.prepare(
            "SELECT rowid, distance FROM embeddings_vec WHERE embedding MATCH ?1 AND k = ?2",
        )?;
        let rows: Vec<(i64, f64)> = stmt
            .query_map(params![blob, k as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::Db)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Keep only embeddings produced by the current model/version.
        let placeholders = vec!["?"; rows.len()].join(",");
        let sql = format!(
            "SELECT memory_id FROM embeddings
             WHERE memory_id IN ({placeholders}) AND model = ? AND model_version = ?"
        );
        let mut params: Vec<rusqlite::types::Value> = rows
            .iter()
            .map(|(id, _)| rusqlite::types::Value::Integer(*id))
            .collect();
        params.push(rusqlite::types::Value::Text(model.to_string()));
        params.push(rusqlite::types::Value::Text(model_version.to_string()));
        let mut stmt = self.conn.prepare(&sql)?;
        let current: std::collections::HashSet<i64> = stmt
            .query_map(rusqlite::params_from_iter(params), |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()
            .map_err(Error::Db)?;

        Ok(rows
            .into_iter()
            .filter(|(id, _)| current.contains(id))
            .collect())
    }

    /// Hybrid search (ADR-0016): reciprocal-rank fusion of the FTS5
    /// keyword candidates and the vec0 semantic candidates. Deterministic:
    /// same inputs, same order. `query_vector: None` degrades to
    /// FTS-only (model unavailable or query embedding failed).
    pub fn hybrid_search(
        &self,
        query: &str,
        query_vector: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<HybridHit>> {
        let mut fused: HashMap<i64, HybridHit> = HashMap::new();

        let fts_hits = self.search(query, CANDIDATE_POOL)?;
        for (pos, hit) in fts_hits.into_iter().enumerate() {
            let contribution = FTS_WEIGHT / (RRF_K + pos as f64 + 1.0);
            fused.insert(
                hit.memory.id,
                HybridHit {
                    memory: hit.memory,
                    fts_rank: Some(contribution),
                    sem_similarity: None,
                    fused_score: contribution,
                },
            );
        }

        if let Some(vector) = query_vector {
            let sem_hits = self.semantic_search(vector, CANDIDATE_POOL, MODEL_ID, MODEL_VERSION)?;
            for (pos, (memory_id, distance)) in sem_hits.into_iter().enumerate() {
                let similarity = (1.0 - distance).clamp(0.0, 1.0);
                let contribution = SEM_WEIGHT / (RRF_K + pos as f64 + 1.0);
                match fused.get_mut(&memory_id) {
                    Some(entry) => {
                        entry.sem_similarity = Some(similarity);
                        entry.fused_score += contribution;
                    }
                    None => {
                        if let Some(memory) = self.get_memory(memory_id)? {
                            fused.insert(
                                memory_id,
                                HybridHit {
                                    memory,
                                    fts_rank: None,
                                    sem_similarity: Some(similarity),
                                    fused_score: contribution,
                                },
                            );
                        }
                    }
                }
            }
        }

        let mut hits: Vec<HybridHit> = fused.into_values().collect();
        hits.sort_by(|a, b| {
            b.fused_score
                .partial_cmp(&a.fused_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory.captured_at.cmp(&a.memory.captured_at))
                .then_with(|| b.memory.id.cmp(&a.memory.id))
        });
        hits.truncate(limit);
        Ok(hits)
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

/// Serialize f32 vector to little-endian bytes (sqlite-vec blob layout).
pub fn to_blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|x| x.to_le_bytes()).collect()
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
