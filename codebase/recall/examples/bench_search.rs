//! Repeatable keyword-search baseline (Phase 6 target: 10,000 entries
//! searched in <100ms — baseline first, per the BloopLab performance
//! standard: Measure → Profile → Optimize → Benchmark → Document).
//!
//! Run:  cargo run --release --example bench_search
//!
//! Seeds a deterministic 10,000-entry dataset into a temp database and
//! measures FTS5 keyword-search latency (avg/min/max over repeated runs).

use std::time::Instant;

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::Db;
use time::{Duration, OffsetDateTime};

const ENTRIES: usize = 10_000;
const ITERATIONS: usize = 20;

const QUERIES: &[&str] = &[
    "postgres connection pool",
    "database is locked",
    "tls handshake timeout",
    "kafka consumer lag",
    "ci pipeline flake",
    "migration missing column",
];

fn main() {
    // Deterministic pseudo-random generator (no rand dependency).
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }
    let mut rng = XorShift(0x5eed_2026_0814);

    let services = [
        "checkout-service",
        "payment-api",
        "auth-service",
        "ingest-worker",
        "report-job",
    ];
    let errors = [
        "postgres connection pool exhausted: too many clients for database appdb",
        "ERROR: relation \"orders\" does not exist (line 42)",
        "sqlite database is locked (code 5): , while compiling: INSERT INTO",
        "TLS handshake timeout to payment-api: deadline exceeded",
        "kafka consumer lag on topic orders-events exceeded 5000",
        "ci pipeline flake: npm ERR! network timeout while fetching dependency",
        "migration failed: missing column \"enabled\" in table settings",
    ];
    let solutions = [
        "raised max_connections and enabled pgbouncer transaction pooling",
        "re-ran the missing migration in staging first",
        "set busy_timeout to 5000ms and moved writes into a single transaction",
        "increased dial timeout and reused the connection pool",
        "scaled consumers and tuned fetch.max.wait.ms",
        "pinned the action version and cached node_modules",
        "generated a follow-up migration that adds the column",
    ];

    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Db::open(&dir.path().join("bench.db")).expect("open bench db");

    let seed_started = Instant::now();
    for _ in 0..ENTRIES {
        let s = (rng.next() as usize) % services.len();
        let e = (rng.next() as usize) % errors.len();
        let memory = NewMemory {
            problem: format!(
                "{} on {}",
                errors[e].split(':').next().unwrap_or("issue"),
                services[s]
            ),
            solution: solutions[e].to_string(),
            error: Some(errors[e].to_string()),
            context: Some(format!("{} v1.2.3", services[s])),
            investigation: Some("checked logs, replayed locally".to_string()),
            project: Some(services[s].to_string()),
            git_commit: Some(format!("{:x}", rng.next())),
            ..Default::default()
        };
        db.insert_memory(
            &memory,
            OffsetDateTime::now_utc() - Duration::days((rng.next() % 365) as i64),
        )
        .expect("insert");
    }
    let seed_ms = seed_started.elapsed().as_millis();

    println!("Recall keyword-search baseline");
    println!("===============================");
    println!("entries:     {ENTRIES}");
    println!("seed time:   {seed_ms} ms ({} inserts)", ENTRIES);
    println!();
    println!("query, iterations, avg_ms, min_ms, max_ms, hits");

    for query in QUERIES {
        let mut times = Vec::with_capacity(ITERATIONS);
        let mut hits = 0;
        for _ in 0..ITERATIONS {
            let started = Instant::now();
            let results = db.search(query, 20).expect("search");
            times.push(started.elapsed().as_secs_f64() * 1000.0);
            hits = results.len();
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        println!(
            "\"{query}\", {ITERATIONS}, {avg:.2}, {:.2}, {:.2}, {hits}",
            times[0],
            times[times.len() - 1]
        );
    }
    println!();
    println!(
        "machine: {} / {} logical cpus",
        std::env::consts::OS,
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
}
