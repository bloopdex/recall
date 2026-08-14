-- 0001_init.sql — initial Recall schema.
--
-- Design: one `memories` table (canonical store) + one external-content FTS5
-- virtual table (`memories_fts`) kept in sync by triggers — the SQLite-native
-- mechanism, so canonical rows are the single source of truth.
-- See ADR-004 (schema) and ADR-005 (FTS5).

CREATE TABLE memories (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    problem            TEXT NOT NULL,
    solution           TEXT NOT NULL,
    error              TEXT,
    context            TEXT,
    investigation      TEXT,
    root_cause         TEXT,
    verification       TEXT,
    environment        TEXT,
    explanation        TEXT,
    project            TEXT,
    repo_path          TEXT,
    git_branch         TEXT,
    git_commit         TEXT,
    git_changed_files  TEXT,
    cwd                TEXT,
    captured_at        TEXT NOT NULL,
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX idx_memories_project     ON memories(project);
CREATE INDEX idx_memories_captured_at ON memories(captured_at);

-- External-content FTS5 index. Column order fixes the bm25() weight
-- positions used by the search query (see database/README.md).
CREATE VIRTUAL TABLE memories_fts USING fts5(
    problem, solution, error, context, investigation,
    root_cause, verification, environment, explanation,
    content = 'memories',
    content_rowid = 'id',
    tokenize = "unicode61 remove_diacritics 1"
);

-- Keep the FTS index synchronized with the canonical table.
CREATE TRIGGER trg_memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(
        rowid, problem, solution, error, context, investigation,
        root_cause, verification, environment, explanation)
    VALUES (
        new.id, new.problem, new.solution, new.error, new.context, new.investigation,
        new.root_cause, new.verification, new.environment, new.explanation);
END;

CREATE TRIGGER trg_memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(
        memories_fts, rowid, problem, solution, error, context, investigation,
        root_cause, verification, environment, explanation)
    VALUES (
        'delete', old.id, old.problem, old.solution, old.error, old.context,
        old.investigation, old.root_cause, old.verification, old.environment,
        old.explanation);
END;

CREATE TRIGGER trg_memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(
        memories_fts, rowid, problem, solution, error, context, investigation,
        root_cause, verification, environment, explanation)
    VALUES (
        'delete', old.id, old.problem, old.solution, old.error, old.context,
        old.investigation, old.root_cause, old.verification, old.environment,
        old.explanation);
    INSERT INTO memories_fts(
        rowid, problem, solution, error, context, investigation,
        root_cause, verification, environment, explanation)
    VALUES (
        new.id, new.problem, new.solution, new.error, new.context, new.investigation,
        new.root_cause, new.verification, new.environment, new.explanation);
END;
