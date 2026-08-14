# ADR-0002 — SQLite library choice

- **Date:** 2026-08-14
- **Status:** Accepted

## Context

Phase 1 needs SQLite + FTS5 from Rust. Options: `rusqlite` (bundled
SQLite), `sqlx` (async, compile-time checked queries), `diesel`
(ORM/query builder), or shelling out to the `sqlite3` CLI.

## Decision

**`rusqlite` with the `bundled` feature** (SQLite compiled into the
binary, FTS5 enabled — verified by the `fts5_probe` test).

## Consequences

- No system-SQLite version drift; CI machines need nothing installed.
- Typed prepared statements; parameterized SQL enforced by API design.
- Single dependency instead of an ORM stack.

## Alternatives considered

- **sqlx** — rejected: async runtime for a sync CLI; SQLite support weaker.
- **diesel** — rejected: ORM ceremony for a 2-table schema.
- **sqlite3 CLI subprocess** — rejected: fragile quoting, no transactions
  across calls, slower.

## Why this choice

Smallest API that gives us migrations, transactions, and FTS5 with
compile-time safety and zero runtime deps.

## Revisit conditions

If Recall grows a server component (not planned), reevaluate with sqlx.
