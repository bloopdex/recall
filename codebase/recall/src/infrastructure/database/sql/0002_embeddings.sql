-- 0002_embeddings.sql — semantic-search enrichment layer (ADR-0015).
--
-- The `embeddings` table is the canonical vector store: one row per
-- memory-model pair, with the raw f32 vector blob plus model metadata.
-- The vec0 virtual table (created at runtime, since it needs the
-- sqlite-vec extension loaded) is a derived index kept in sync by
-- triggers, exactly like the FTS5 layer in 0001.
--
-- Enrichment is optional: a memory without an embedding row remains
-- fully searchable via FTS5, editable, and listable.

CREATE TABLE embeddings (
    memory_id     INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    model         TEXT NOT NULL,
    model_version TEXT NOT NULL,
    dims          INTEGER NOT NULL,
    vector        BLOB NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX idx_embeddings_model ON embeddings(model, model_version);
