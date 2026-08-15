//! Project-scoped search baseline (Phase 5, ADR-0022): 10,000 memories
//! across 10 projects, synthetic vectors. Measures global vs
//! project-scoped FTS, semantic, and hybrid latency — the point is to
//! verify that the WHERE-clause filtering rides on the existing indexes
//! without regressing the Phase 3 numbers.
//!
//! Run:  cargo run --release --example bench_projects

use std::time::Instant;

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::{Db, SearchFilter};
use recall::infrastructure::embeddings::EMBED_DIMS;
use time::{Duration, OffsetDateTime};

const ENTRIES: usize = 10_000;
const PROJECTS: usize = 10;
const ITERATIONS: usize = 20;

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
    let mut rng = XorShift(0x5eed_2026_0815);

    let errors = [
        "postgres connection pool exhausted: too many clients for database appdb",
        "ERROR: relation \"orders\" does not exist (line 42)",
        "sqlite database is locked (code 5)",
        "TLS handshake timeout to payment-api: deadline exceeded",
        "kafka consumer lag on topic orders-events exceeded 5000",
        "ci pipeline flake: npm ERR! network timeout",
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
    let mut vectors = Vec::with_capacity(ENTRIES);
    for _ in 0..ENTRIES {
        let project = format!("project-{:02}", (rng.next() as usize) % PROJECTS);
        let e = (rng.next() as usize) % errors.len();
        let memory = NewMemory {
            problem: format!(
                "{} on {}",
                errors[e].split(':').next().unwrap_or("issue"),
                project
            ),
            solution: solutions[e].to_string(),
            error: Some(errors[e].to_string()),
            project: Some(project),
            ..Default::default()
        };
        let id = db
            .insert_memory(
                &memory,
                OffsetDateTime::now_utc() - Duration::days((rng.next() % 365) as i64),
            )
            .expect("insert");
        // Synthetic deterministic vector (latency is data-driven here;
        // quality is measured by the eval harness).
        let v: Vec<f32> = (0..EMBED_DIMS)
            .map(|i| ((rng.next() % 1_000_000) as f32 + i as f32) * 1e-6)
            .collect();
        let l: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let v: Vec<f32> = v.iter().map(|x| x / l).collect();
        db.insert_embedding(id, "bench", "1", EMBED_DIMS, &v)
            .expect("embed");
        vectors.push(v);
    }
    let seed_ms = seed_started.elapsed().as_millis();

    let query_vec = &vectors[0];
    let scoped = SearchFilter::with_project("project-03");

    println!("Recall project-scoped search baseline");
    println!("====================================");
    println!("entries:  {ENTRIES} across {PROJECTS} projects (seed {seed_ms} ms)");
    println!();
    println!("engine, scope, avg_ms, min_ms, max_ms");

    let report = |label: &str, f: &dyn Fn()| {
        let mut times = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let t = Instant::now();
            f();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "{label}, {:.2}, {:.2}, {:.2}",
            times.iter().sum::<f64>() / ITERATIONS as f64,
            times[0],
            times[ITERATIONS - 1]
        );
    };

    for (label, filter) in [
        ("FTS, global", &SearchFilter::default()),
        ("FTS, project-03", &scoped),
    ] {
        report(label, &|| {
            db.search_filtered("connection pool", filter, 20).unwrap();
        });
    }
    for (label, filter) in [
        ("semantic, global", &SearchFilter::default()),
        ("semantic, project-03", &scoped),
    ] {
        report(label, &|| {
            db.semantic_search(query_vec, 50, "bench", "1", filter)
                .unwrap();
        });
    }
    for (label, filter) in [
        ("hybrid, global", &SearchFilter::default()),
        ("hybrid, project-03", &scoped),
    ] {
        report(label, &|| {
            db.hybrid_search("connection pool", Some(query_vec), filter, 20)
                .unwrap();
        });
    }
}
