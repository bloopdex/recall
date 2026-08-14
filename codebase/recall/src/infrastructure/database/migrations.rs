//! Versioned migration system (ADR-006).
//!
//! Migrations are embedded SQL files applied in version order inside
//! individual transactions, tracked in `schema_migrations`. The schema is
//! never created ad hoc at runtime.

use rusqlite::{params, Connection, Transaction};

use crate::{Error, Result};

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Ordered, immutable migration list. Append-only: never edit an applied
/// migration — add a new version instead.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("sql/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "embeddings",
        sql: include_str!("sql/0002_embeddings.sql"),
    },
];

/// Apply all pending migrations to `conn`. Each migration runs in its own
/// transaction so a failure cannot leave a half-applied schema.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    debug_assert!(
        MIGRATIONS.windows(2).all(|w| w[0].version < w[1].version),
        "MIGRATIONS must be sorted by version"
    );

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         );",
    )?;

    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    for migration in MIGRATIONS.iter().filter(|m| m.version > current) {
        apply(conn, migration)?;
    }
    Ok(())
}

fn apply(conn: &mut Connection, migration: &Migration) -> Result<()> {
    let tx: Transaction<'_> = conn.transaction()?;
    tx.execute_batch(migration.sql).map_err(|e| {
        Error::Migration(format!(
            "migration {} ({}) failed: {e}",
            migration.version, migration.name
        ))
    })?;
    tx.execute(
        "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
        params![migration.version, migration.name],
    )?;
    tx.commit()?;
    Ok(())
}
