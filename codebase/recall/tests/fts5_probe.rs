//! Build-time probe: confirms the bundled SQLite has FTS5 enabled.
//! If this fails, the FTS5 flag must be enabled for the bundled build
//! (documented in ADR-002/ADR-005).

#[test]
fn bundled_sqlite_has_fts5() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE VIRTUAL TABLE probe USING fts5(content, tokenize = 'unicode61');
         INSERT INTO probe(rowid, content) VALUES (1, 'postgres connection pool exhausted');",
    )
    .expect("FTS5 must be available in the bundled SQLite");
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM probe WHERE probe MATCH ?1",
            ["connection"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}
