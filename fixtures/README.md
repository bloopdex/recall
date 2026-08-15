# Fixtures

Reference fixture data for Recall.

- `example_entries.sql` — the three realistic example entries from the
  schema validation (PostgreSQL pool exhaustion, SQLite lock, missing
  table). Loadable into any recall database via the sqlite3 CLI;
  recall's triggers keep the FTS5 index in sync automatically.
- `upgrade/pre-release-export.json` — a hand-written export fixture from
  the pre-release development, pinned by `tests/upgrade_paths.rs` to keep
  old export files importable (ADR-0024).
