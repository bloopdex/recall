-- 0003: memory lifecycle status (ADR-0023).
--
-- A memory is `active` (searchable everywhere) or `archived` (kept,
-- excluded from search by default, recoverable via `recall unarchive`).
-- No CHECK constraint: values are only ever set by Recall's own code, and
-- a CHECK would force a migration for any future state.
ALTER TABLE memories ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
