# Fixtures

Reference fixture data for Recall.

- `example_entries.sql` — the three realistic example entries from the
  Phase 0 schema validation (PostgreSQL pool exhaustion, SQLite lock,
  missing table). Loadable into any recall database via the sqlite3 CLI;
  recall's triggers keep the FTS5 index in sync automatically.

Later phases will grow this directory with search-quality corpora
(Phase 3) and benchmark datasets (Phase 6).
