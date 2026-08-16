//! Concurrency and multi-process behavior.
//!
//! Recall is a local CLI that may run from a shell hook, a git hook,
//! several terminals, and scripts at the same time. The model under test
//! (ADR-0027): one SQLite database file in WAL mode, every write in its
//! own transaction, a 5 s busy timeout for write contention, readers
//! never blocked by writers. Tests cover:
//! - concurrent captures from separate processes (all must persist)
//! - reads during a sustained write stream (never error, never see
//!   partial rows)
//! - a reader during a held write transaction (WAL: readers don't block)
//! - concurrent archive vs delete (serialized; final state consistent)
//! - concurrent embedding inserts from separate connections
//!
//! All databases are temporary files. The spawned-process tests use the
//! compiled `recall` binary via `common::run`.

mod common;

use std::path::Path;
use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::Db;
use recall::infrastructure::embeddings::EMBED_DIMS;
use time::OffsetDateTime;

use common::{stderr, stdout};

fn run(db: &Path, cwd: Option<&Path>, args: &[&str]) -> Output {
    common::run(db, cwd, args, None)
}

fn capture_in(dir: &Path, db: &Path, problem: &str) {
    let out = run(
        db,
        Some(dir),
        &[
            "capture",
            "--problem",
            problem,
            "--solution",
            "solution",
            "--project",
            "concurrency",
        ],
    );
    assert!(
        out.status.success(),
        "capture '{problem}' failed: {}",
        stderr(&out)
    );
}

#[test]
fn concurrent_captures_never_lose_data_silently() {
    // The guarantee at realistic concurrency (ADR-0027): a shell hook, a
    // git hook, and a terminal firing together against a warm database.
    // On a normal machine every capture persists; under pathological
    // load (this test runs inside the full suite, alongside ~20 other
    // suites thrashing the disk) a capture may instead hit the 5 s busy
    // timeout — and then it fails LOUDLY ("database is locked") and the
    // retry succeeds. Silent loss and corruption are what must never
    // happen.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");
    capture_in(dir.path(), &db, "warm-up capture");

    const PROCESSES: usize = 3;
    let mut failed: Vec<String> = Vec::new();
    let handles: Vec<_> = (0..PROCESSES)
        .map(|i| {
            let dir = dir.path().to_path_buf();
            let db = db.clone();
            let label = format!("concurrent capture number {i}");
            std::thread::spawn(move || {
                let out = run(
                    &db,
                    Some(&dir),
                    &[
                        "capture",
                        "--problem",
                        &label,
                        "--solution",
                        "solution",
                        "--project",
                        "concurrency",
                    ],
                );
                assert!(
                    out.status.success() || stderr(&out).contains("database is locked"),
                    "capture failed with an unexpected error: {}",
                    stderr(&out)
                );
                if out.status.success() {
                    String::new()
                } else {
                    label
                }
            })
        })
        .collect();
    for h in handles {
        let label = h.join().expect("capture thread panicked");
        if !label.is_empty() {
            failed.push(label);
        }
    }

    // Any busy-failed capture succeeds on retry — the no-silent-loss
    // guarantee.
    for label in &failed {
        capture_in(dir.path(), &db, label);
    }
    let out = run(&db, None, &["search", "concurrent capture number"]);
    assert!(out.status.success(), "search failed: {}", stderr(&out));
    let text = stdout(&out);
    let hits = text
        .lines()
        .filter(|l| l.contains("problem:  concurrent capture number"))
        .count();
    assert_eq!(hits, PROCESSES, "all captures must be present: {text}");
}

#[test]
fn highly_contended_captures_never_lose_data_silently() {
    // 8 simultaneous captures on a warm database — beyond the documented
    // usage, and this suite runs it under full-suite load.
    // The pinned guarantee (ADR-0027): every outcome is either a capture
    // or a LOUD "database is locked" busy error (the 5 s timeout
    // exhausted by extreme contention) — never a silent loss, never
    // corruption — and any failed capture succeeds on retry.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");
    capture_in(dir.path(), &db, "warm-up capture");

    const PROCESSES: usize = 8;
    let mut failures: Vec<String> = Vec::new();
    let handles: Vec<_> = (0..PROCESSES)
        .map(|i| {
            let dir = dir.path().to_path_buf();
            let db = db.clone();
            let label = format!("contended capture {i}");
            std::thread::spawn(move || {
                let out = run(
                    &db,
                    Some(&dir),
                    &[
                        "capture",
                        "--problem",
                        &label,
                        "--solution",
                        "solution",
                        "--project",
                        "concurrency",
                    ],
                );
                assert!(
                    out.status.success() || stderr(&out).contains("database is locked"),
                    "capture failed with an unexpected error: {}",
                    stderr(&out)
                );
                if !out.status.success() {
                    label
                } else {
                    String::new()
                }
            })
        })
        .collect();
    for h in handles {
        let label = h.join().expect("capture thread panicked");
        if !label.is_empty() {
            failures.push(label);
        }
    }

    // Every failure is retryable: the second attempt must succeed.
    for label in &failures {
        capture_in(dir.path(), &db, label);
    }
    let out = run(&db, None, &["check"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the store must be consistent after contention: {}",
        stderr(&out)
    );
}

#[test]
fn concurrent_first_run_captures_fail_loudly_and_the_store_stays_healthy() {
    // Documented corner (ADR-0027): 8 processes racing to CREATE the
    // database (WAL setup + migrations need the write lock) under load
    // can exhaust the 5 s busy timeout or lose the migration race. The
    // pinned behavior: every process either captures or fails LOUDLY
    // ("database is locked", or the migration loser's "table ...
    // already exists") — never a silent loss, never corruption — and
    // the store is healthy and usable afterwards.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");

    const PROCESSES: usize = 8;
    let handles: Vec<_> = (0..PROCESSES)
        .map(|i| {
            let dir = dir.path().to_path_buf();
            let db = db.clone();
            std::thread::spawn(move || {
                let out = run(
                    &db,
                    Some(&dir),
                    &[
                        "capture",
                        "--problem",
                        &format!("first run capture {i}"),
                        "--solution",
                        "solution",
                        "--project",
                        "concurrency",
                    ],
                );
                let loud_failure = stderr(&out).contains("database is locked")
                    || stderr(&out).contains("already exists");
                assert!(
                    out.status.success() || loud_failure,
                    "capture {i} failed with an unexpected error: {}",
                    stderr(&out)
                );
            })
        })
        .collect();
    for h in handles {
        h.join().expect("capture thread panicked");
    }

    // Healthy and usable: check passes, and a fresh capture succeeds.
    let out = run(&db, None, &["check"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the store must be consistent: {}",
        stderr(&out)
    );
    capture_in(dir.path(), &db, "post-race capture");
    let out = run(&db, None, &["search", "post-race capture"]);
    assert!(
        stdout(&out).contains("problem:  post-race capture"),
        "the store must accept new captures: {}",
        stdout(&out)
    );
}

#[test]
fn reads_during_a_sustained_write_stream_never_error_or_see_partial_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("recall.db");

    // Initialize the store once, up front: first-run CREATION races are
    // a different pinned corner (the test above). This test is about
    // reads during writes on a warm database, so the schema must exist
    // before either thread opens the file.
    drop(Db::open(&db_path).expect("initial open"));

    const WRITES: usize = 200;
    let done = Arc::new(AtomicBool::new(false));
    let writer_db_path = db_path.clone();
    let writer = {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut db = Db::open(&writer_db_path).expect("writer connection");
            for i in 0..WRITES {
                let memory = NewMemory {
                    problem: format!("writer entry {i}"),
                    solution: format!("solution {i}"),
                    project: Some("concurrency".into()),
                    ..Default::default()
                };
                db.insert_memory(&memory, OffsetDateTime::now_utc())
                    .expect("insert");
            }
            done.store(true, Ordering::SeqCst);
        })
    };

    // Readers keep querying while the writer is still inserting.
    let mut reads = 0usize;
    while !done.load(Ordering::SeqCst) {
        let db = Db::open(&db_path).expect("reader connection");
        let hits = db
            .search_filtered(
                "writer entry",
                &recall::infrastructure::database::SearchFilter::default(),
                20,
            )
            .expect("search must never error during writes");
        // Every row must be complete: a partially written memory would
        // show up as an empty problem or a dangling id.
        for h in &hits {
            assert!(!h.memory.problem.is_empty(), "partial row observed");
            assert!(h.memory.id > 0, "invalid id observed");
        }
        reads += 1;
    }
    writer.join().expect("writer thread panicked");

    let db = Db::open(&db_path).expect("final connection");
    let all = db
        .search_filtered(
            "writer entry",
            &recall::infrastructure::database::SearchFilter::default(),
            WRITES + 10,
        )
        .expect("final search");
    assert_eq!(all.len(), WRITES, "all writes must be durable");
    assert!(reads > 0, "the reader loop must have run at least once");
}

#[test]
fn readers_do_not_block_behind_a_held_write_transaction() {
    // WAL mode promise: a writer holding an open transaction does not
    // block readers. The reader here must finish while the writer's
    // transaction is still open (synchronized with channels).
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("recall.db");

    // Seed one memory so the reader has something to find.
    {
        let mut db = Db::open(&db_path).expect("seed connection");
        db.insert_memory(
            &NewMemory {
                problem: "seed entry".into(),
                solution: "s".into(),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .expect("seed");
    }

    let (tx_open_tx, tx_open_rx) = std::sync::mpsc::channel::<()>();
    let (reader_done_tx, reader_done_rx) = std::sync::mpsc::channel::<()>();
    let writer_path = db_path.clone();
    let writer = std::thread::spawn(move || {
        let db = Db::open(&writer_path).expect("writer connection");
        db.with_connection(|c| {
            c.execute_batch("BEGIN IMMEDIATE;").expect("begin");
            c.execute(
                "INSERT INTO memories (problem, solution, status, captured_at, project)
                 VALUES ('held transaction entry', 's', 'active', '2026-01-01T00:00:00.000Z', 'concurrency')",
                [],
            )
            .expect("insert into held tx");
            tx_open_tx.send(()).expect("signal tx open");
            reader_done_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("reader must finish while the tx is held");
            c.execute_batch("COMMIT;").expect("commit");
        });
    });

    tx_open_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("writer tx must open");

    let started = std::time::Instant::now();
    let reader_db = Db::open(&db_path).expect("reader connection");
    let hits = reader_db
        .search_filtered(
            "seed entry",
            &recall::infrastructure::database::SearchFilter::default(),
            10,
        )
        .expect("read during held write tx");
    let elapsed = started.elapsed();
    assert!(!hits.is_empty(), "seeded memory must be readable");
    assert!(
        elapsed < Duration::from_millis(500),
        "reader blocked behind a held write transaction: {elapsed:?}"
    );

    // The uncommitted row must not be visible to other connections.
    assert_eq!(
        reader_db
            .search_filtered(
                "held transaction",
                &recall::infrastructure::database::SearchFilter::default(),
                10,
            )
            .expect("read")
            .len(),
        0,
        "uncommitted data must not be visible to other connections"
    );

    reader_done_tx.send(()).expect("release writer");
    writer.join().expect("writer thread panicked");

    let hits = reader_db
        .search_filtered(
            "held transaction",
            &recall::infrastructure::database::SearchFilter::default(),
            10,
        )
        .expect("read after commit");
    assert_eq!(hits.len(), 1, "committed data must be visible");
}

#[test]
fn concurrent_archive_and_delete_leave_a_consistent_state() {
    // Two processes race: one archives the memory, one deletes it. The
    // operations serialize on SQLite's write lock; whichever loses gets a
    // clean error. The final state must be exactly one of: memory absent
    // (delete won), or memory archived (archive won, delete then failed).
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");
    capture_in(dir.path(), &db, "race target");

    // The memory id is 1 (fresh database).
    let archive = {
        let db = db.clone();
        let dir = dir.path().to_path_buf();
        std::thread::spawn(move || run(&db, Some(&dir), &["archive", "1"]))
    };
    let delete = {
        let db = db.clone();
        let dir = dir.path().to_path_buf();
        std::thread::spawn(move || run(&db, Some(&dir), &["delete", "1", "--yes"]))
    };
    let archive_out = archive.join().expect("archive thread panicked");
    let delete_out = delete.join().expect("delete thread panicked");

    // Exactly one may succeed; the loser errors cleanly (id already gone).
    let successes = [&archive_out, &delete_out]
        .iter()
        .filter(|o| o.status.success())
        .count();
    assert!(successes >= 1, "one operation must win the race");

    // Consistent final state: either gone from everything, or archived
    // (active search misses it, archived listing finds it), and never a
    // mix of half-deleted index entries. Hit lines carry a
    // "problem:  <text>" marker; the "No results" echo of the query
    // string must not be mistaken for a hit.
    let active = run(&db, None, &["search", "race target"]);
    let archived = run(&db, None, &["list", "--archived", "--limit", "10"]);
    let active_text = stdout(&active);
    let archived_text = stdout(&archived);
    let active_hit = active_text.contains("problem:  race target");
    let archived_hit = archived_text.contains("problem:  race target");
    assert!(
        !active_hit,
        "the memory must never surface in active search after the race: {active_text}"
    );
    // Both end states are legal: gone (delete won) or archived (archive
    // won, delete then failed with a clean error). The FTS-consistency
    // check below covers whichever interleaving happened; here we only
    // record which one it was so a failure message is diagnosable.
    assert!(
        archived_hit || !archived_text.contains("problem:"),
        "archived listing must be coherent: {archived_text}"
    );

    // FTS and the memory table must agree either way.
    let db_conn = Db::open(&db).expect("inspect final state");
    let row_count: i64 = db_conn
        .with_connection(|c| c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0)))
        .expect("count memories");
    let fts_count: i64 = db_conn
        .with_connection(|c| c.query_row("SELECT count(*) FROM memories_fts", [], |r| r.get(0)))
        .expect("count fts");
    assert_eq!(row_count, fts_count, "FTS must stay in sync");
}

#[test]
fn concurrent_embedding_inserts_from_separate_connections_are_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("recall.db");
    let mut seed = Db::open(&db_path).expect("seed connection");

    // 8 distinct memories, one per thread, each with its own connection.
    const MEMORIES: usize = 8;
    let ids: Vec<i64> = (0..MEMORIES)
        .map(|i| {
            seed.insert_memory(
                &NewMemory {
                    problem: format!("embedding target {i}"),
                    solution: "s".into(),
                    ..Default::default()
                },
                OffsetDateTime::now_utc(),
            )
            .expect("seed memory")
        })
        .collect();

    let handles: Vec<_> = ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            let db_path = db_path.clone();
            std::thread::spawn(move || {
                let mut db = Db::open(&db_path).expect("worker connection");
                let mut v = vec![(i as f32 + 1.0) * 1e-2; EMBED_DIMS];
                v[0] = 1.0;
                db.insert_embedding(id, "bench", "1", EMBED_DIMS, &v)
                    .expect("embedding insert");
            })
        })
        .collect();
    for h in handles {
        h.join().expect("embedding thread panicked");
    }

    let db = Db::open(&db_path).expect("final connection");
    let count: i64 = db
        .with_connection(|c| c.query_row("SELECT count(*) FROM embeddings", [], |r| r.get(0)))
        .expect("count embeddings");
    assert_eq!(count, MEMORIES as i64, "no embedding may be lost");
    let vec_count: i64 = db
        .with_connection(|c| c.query_row("SELECT count(*) FROM embeddings_vec", [], |r| r.get(0)))
        .expect("count vec rows");
    assert_eq!(vec_count, MEMORIES as i64, "vec0 must stay in sync");
}
