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

    /// Phase 6: the full upgrade path from the original schema (v1, before
    /// embeddings existed). Both pending migrations apply in one open, the
    /// data survives, and the post-upgrade surface (embeddings, lifecycle)
    /// works on the old row.
    #[test]
    fn upgrading_a_v1_database_applies_both_migrations_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recall.db");

        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("sql/0001_init.sql"))
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                 version    INTEGER PRIMARY KEY,
                 name       TEXT NOT NULL,
                 applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             );
             INSERT INTO schema_migrations (version, name) VALUES (1, 'init');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (problem, solution, captured_at)
             VALUES ('v1 era memory', 'old fix', '2026-08-13T10:00:00.000Z')",
            [],
        )
        .unwrap();
        drop(conn);

        let mut db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 3);

        let memories = db.list_memories(10).unwrap();
        assert_eq!(memories.len(), 1, "v1 data must survive to v3");
        assert_eq!(memories[0].problem, "v1 era memory");
        assert_eq!(memories[0].status, MemoryStatus::Active);

        // Post-upgrade surfaces work on the migrated row: embedding insert
        // (migration 2) and lifecycle (migration 3).
        let mut v = vec![0.1f32; crate::infrastructure::embeddings::EMBED_DIMS];
        v[0] = 1.0;
        db.insert_embedding(
            memories[0].id,
            "bench",
            "1",
            crate::infrastructure::embeddings::EMBED_DIMS,
            &v,
        )
        .unwrap();
        assert!(db
            .set_status(memories[0].id, MemoryStatus::Archived)
            .unwrap());
        assert_eq!(
            db.get_memory(memories[0].id).unwrap().unwrap().status,
            MemoryStatus::Archived
        );
    }

    /// Phase 6: a failing migration must not silently destroy anything.
    /// The failure rolls back the migration's transaction (no partial
    /// schema), is not recorded in `schema_migrations`, leaves the
    /// pre-existing file intact, and is retryable once the conflict is
    /// resolved.
    #[test]
    fn a_failed_migration_is_atomic_and_retryable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recall.db");

        // v2 database, then simulate an external tool having already added
        // the column migration 3 wants to add (a realistic conflict).
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
             VALUES ('conflict survivor', 's', '2026-08-14T10:00:00.000Z')",
            [],
        )
        .unwrap();
        conn.execute_batch("ALTER TABLE memories ADD COLUMN status TEXT")
            .unwrap();
        drop(conn);

        let err = Db::open(&path).unwrap_err().to_string();
        assert!(
            err.contains("migration") && err.contains("failed"),
            "the failure must be reported as a migration error: {err}"
        );

        // A backup of the pre-upgrade state was taken before the attempt.
        let backup = PathBuf::from(format!("{}.pre-migration-backup", path.display()));
        assert!(
            backup.exists(),
            "the backup must exist even though the migration failed"
        );

        // The failed migration must not be recorded, the conflict is
        // untouched, and the data survives.
        let probe = Connection::open(&path).unwrap();
        let max_version: i64 = probe
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(max_version, 2, "the failed migration must not be recorded");
        let rows: i64 = probe
            .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "existing data must survive the failed migration");
        drop(probe);

        // Retryable: resolve the conflict, re-open — migration 3 applies.
        let probe = Connection::open(&path).unwrap();
        probe
            .execute_batch("ALTER TABLE memories DROP COLUMN status")
            .unwrap();
        drop(probe);
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 3);
        let memories = db.list_memories(10).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].problem, "conflict survivor");
    }

    /// Phase 6: the documented recovery model, exercised end to end —
    /// destroy the database, restore the pre-migration backup, reopen:
    /// the data is back and the upgrade re-applies cleanly.
    #[test]
    fn restoring_the_pre_migration_backup_recovers_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recall.db");

        // v2 database with one row; opening upgrades it and snapshots the
        // pre-upgrade state.
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
             VALUES ('backup me', 's', '2026-08-14T10:00:00.000Z')",
            [],
        )
        .unwrap();
        drop(conn);

        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 3);
        drop(db);

        let backup = PathBuf::from(format!("{}.pre-migration-backup", path.display()));
        assert!(backup.exists());

        // Destroy the main database.
        std::fs::write(&path, b"this is no longer a database").unwrap();

        // Restore the snapshot: copy the backup over the main file.
        std::fs::copy(&backup, &path).unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 3);
        let memories = db.list_memories(10).unwrap();
        assert_eq!(memories.len(), 1, "the restored data must be intact");
        assert_eq!(memories[0].problem, "backup me");
    }
}
