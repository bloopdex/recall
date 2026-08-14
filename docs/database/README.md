# Recall — Database

## Schema (migration 0001)

Single canonical table `memories` + external-content FTS5 table
`memories_fts` kept in sync by triggers (insert/update/delete). Canonical
rows are the source of truth; the FTS index can always be rebuilt from them.

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| problem | TEXT NOT NULL | required user input |
| solution | TEXT NOT NULL | required user input |
| error | TEXT NULL | exact symptom/error message |
| context | TEXT NULL | environment/versions at incident time |
| investigation | TEXT NULL | commands + relevant files |
| root_cause | TEXT NULL | why it happened |
| verification | TEXT NULL | how the fix was verified |
| environment | TEXT NULL | environment metadata |
| explanation | TEXT NULL | free-form elaboration |
| project | TEXT NULL | auto-detected or `--project` |
| repo_path | TEXT NULL | git top-level, auto |
| git_branch | TEXT NULL | auto (None when detached) |
| git_commit | TEXT NULL | short SHA, auto |
| git_changed_files | TEXT NULL | `git status --porcelain`, capped 50 lines |
| cwd | TEXT NULL | auto |
| captured_at | TEXT NOT NULL | RFC3339 UTC, millisecond precision |
| created_at / updated_at | TEXT NOT NULL | DB defaults |

Indexes: `project`, `captured_at` (list + future project scoping).

Phase 0 model mapping: Error→`error`, Context→`context`, Commands/Relevant
files→`investigation`, Git commit→`git_commit`, Solution→`solution`,
Project→`project`, Timestamp→`captured_at`, Optional explanation→
`explanation`. `problem`, `root_cause`, `verification`, `environment` and the
extra git fields are the Phase 1/2 spec extensions (ADR-0004).

## FTS5 design (ADR-0005)

- **Table:** external-content (`content='memories'`) — one canonical copy.
- **Columns:** problem, solution, error, context, investigation, root_cause,
  verification, environment, explanation.
- **Tokenizer:** `unicode61 remove_diacritics 1` — sane defaults for English
  error text; diacritics normalized (café → cafe).
- **Query normalization:** user input is split into terms; each term is
  wrapped as a quoted FTS5 string literal (embedded quotes doubled), terms
  joined with implicit AND. Malformed-query injection is impossible by
  construction; punctuation-only queries are rejected with a clear message.
- **Ranking:** `bm25(memories_fts, 5.0, 3.0, 5.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0)`
  — problem (5.0) and error (5.0) are the strongest signals (exact error
  messages are excellent search keys), solution (3.0) next, background
  fields 1.0. Lower bm25 = better. Ties break by `captured_at DESC`.
- **Sync:** triggers on insert/update/delete of `memories` — the SQLite-
  native mechanism, atomic with the row change.

## PRAGMAs (ADR-0003)

| Setting | Value | Why |
|---|---|---|
| foreign_keys | ON | correct by default (future tables) |
| journal_mode | WAL | readers never block the writer; crash-safe |
| synchronous | NORMAL | safe with WAL for a single-process CLI; FULL buys nothing here |
| busy_timeout | 5000 ms | two terminals opening the DB must not crash |

## Migration strategy (ADR-0006)

Embedded SQL files (`src/infrastructure/database/sql/NNNN_name.sql`) applied
in version order, one transaction each, recorded in `schema_migrations`.
Append-only: never edit an applied migration.

## Semantic layer (Phase 3 — ADR-0014/0015)

- **`embeddings` table** (migration 0002): `memory_id` (PK, FK CASCADE),
  `model`, `model_version`, `dims`, `vector` (little-endian f32 BLOB),
  `created_at`. The canonical vector store.
- **`embeddings_vec`** vec0 virtual table (`float[384]`, cosine distance,
  rowid = memory_id) — a derived index synced by triggers, created at
  open when the sqlite-vec extension loads (via `sqlite3_auto_extension`
  before connection creation).
- **Query shape:** the MATCH must drive the vec0 scan — metadata filtering
  happens as a second lookup over the ≤k rowids, because SQLite may
  reorder a joined plan at scale and emit NULL `distance` (pinned by
  `tests/semantic_10k.rs`). NaN/Inf vectors are rejected at insert.
- Extension load failure degrades to keyword-only search (`vec_enabled`
  false), never a crash.
