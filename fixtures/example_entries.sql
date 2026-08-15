-- Example entries from the schema validation (fixture corpus).
-- These are reference fixtures for manual testing and future test suites.
-- Load into an existing recall database with the sqlite3 CLI:
--   sqlite3 recall.db < fixtures/example_entries.sql
--
-- Note: recall's own triggers keep the FTS index in sync, so plain SQL
-- inserts work exactly like `recall capture`.

-- 1. PostgreSQL connection pool exhaustion (the canonical example)
INSERT INTO memories (
    problem, solution, error, context, investigation, root_cause,
    project, git_branch, git_commit, captured_at
) VALUES (
    'PostgreSQL connection pool exhaustion on checkout-service',
    'Raised max_connections to 200 and enabled pgbouncer transaction pooling in front of the API pods.',
    'connection pool exhausted: too many clients for database appdb',
    'checkout-service v2.4.1, PostgreSQL 16, Node pool size 10 default',
    'Checked pg_stat_activity; found idle-in-transaction sessions held by a cron job.',
    'A nightly cron held idle-in-transaction sessions while the pool defaulted to 10.',
    'thorn-api',
    'main',
    'a1b2c3d',
    '2026-08-13T21:40:00.000Z'
);

-- 2. SQLite "database is locked" (the pipe-friendly example)
INSERT INTO memories (
    problem, solution, error, context, investigation, root_cause,
    project, captured_at
) VALUES (
    'sqlite database is locked during migration',
    'Set busy_timeout to 5000ms and moved the migration + seed into one transaction.',
    'sqlite database is locked (code 5): , while compiling: INSERT INTO schema_migrations',
    'Local dev, sqlite3 CLI open in another terminal',
    'Reproduced with two writers; the second writer had no busy timeout.',
    'Two concurrent writers with the default 0ms busy_timeout.',
    'recall',
    '2026-08-13T22:10:00.000Z'
);

-- 3. Missing table after deploy (the error-message-as-signal example)
INSERT INTO memories (
    problem, solution, error, investigation, verification, project, captured_at
) VALUES (
    'Missing orders table in staging after deploy',
    'Ran the pending Prisma migration manually, then added migrate deploy to the container start command.',
    'ERROR: relation "orders" does not exist (line 42)',
    'Compared local and staging migration state; staging had skipped one migration.',
    'Staging smoke tests green; deploy re-run cleanly.',
    'thorn-api',
    '2026-08-14T09:05:00.000Z'
);
