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
    Migration {
        version: 3,
        name: "lifecycle_status",
        sql: include_str!("sql/0003_lifecycle_status.sql"),
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

#[cfg(test)]
mod tests {
    use crate::domain::memory::MemoryStatus;
    use crate::infrastructure::database::Db;
    use rusqlite::Connection;
    use std::path::PathBuf;

    /// The Phase 5 DoD migration test: upgrading a database at schema v2
    /// (seeded with real data) applies migration 0003 without data loss
    /// and leaves a pre-migration backup with the old schema.
    #[test]
    fn upgrading_a_v2_database_preserves_data_and_creates_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recall.db");

        // Build a v2 database by hand: schema as of migration 2, one row.
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("sql/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("sql/0002_embeddings.sql"))
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                 version    INTEGER PRIMARY KEY,
                 name       TEXT NOT NULL,
                 applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             );
             INSERT INTO schema_migrations (version, name) VALUES (1, 'init'), (2, 'embeddings');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (problem, solution, captured_at)
             VALUES ('old pool exhaustion', 'raised the pool', '2026-08-14T10:00:00.000Z')",
            [],
        )
        .unwrap();
        drop(conn);

        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 3);

        let backup = PathBuf::from(format!("{}.pre-migration-backup", path.display()));
        assert!(backup.exists(), "a pre-migration backup must exist");

        let memories = db.list_memories(10).unwrap();
        assert_eq!(memories.len(), 1, "existing data must survive the upgrade");
        assert_eq!(memories[0].status, MemoryStatus::Active);
        assert_eq!(memories[0].problem, "old pool exhaustion");

        // The backup predates migration 3: its schema has no status column.
        let probe = Connection::open(&backup).unwrap();
        let has_status: i64 = probe
            .query_row(
                "SELECT count(*) FROM pragma_table_info('memories') WHERE name = 'status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_status, 0, "the backup must hold the pre-upgrade schema");
    }
}
