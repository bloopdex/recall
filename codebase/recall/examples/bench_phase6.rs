//! Phase 6 hardening benchmark: every operation at 10k / 50k / 100k
//! entries, with percentile distributions (avg / median / p95 / p99 /
//! min / max), a project/archive mix, and a capture cost breakdown.
//!
//! Run:  cargo run --release --example bench_phase6 -- [size]
//!         size: 10000 (default) | 50000 | 100000
//!       EMBED=0 cargo run ...   → seed WITHOUT embeddings (FTS-only axis)
//!
//! Methodology (ADR-0025):
//! - deterministic XorShift dataset: 10 projects, ~10% archived
//! - synthetic vectors for search latency (embedding QUALITY is measured
//!   by the eval harness; model THROUGHPUT by bench_embed)
//! - 1 warm-up + N measured iterations per cell (import/export at large
//!   sizes run fewer iterations — they are O(N) and slow by design)
//! - application-layer ops timed in-process; process startup measured
//!   separately by spawning the release binary
//!
//! Search-latency numbers do NOT include model inference or process
//! startup — they isolate the database work (the layer Phase 6 targets).

use std::time::Instant;

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::{Db, SearchFilter};
use recall::infrastructure::embeddings::EMBED_DIMS;
use time::{Duration, OffsetDateTime};

const PROJECTS: usize = 10;
const ITERATIONS: usize = 21; // 1 warm-up + 20 measured

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

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx]
}

/// Run the closure `iterations` times (first call is warm-up), print one
/// CSV row: label, n, avg, median, p95, p99, min, max (all ms).
fn report(label: &str, iterations: usize, f: &mut dyn FnMut()) {
    let mut times = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t = Instant::now();
        f();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.remove(0); // drop warm-up
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "{label}, {}, {avg:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}",
        times.len(),
        percentile(&times, 50.0),
        percentile(&times, 95.0),
        percentile(&times, 99.0),
        times[0],
        times[times.len() - 1]
    );
}

fn main() {
    let entries: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let embed = std::env::var("EMBED").map(|v| v != "0").unwrap_or(true);
    let heavy_iterations = if entries >= 50_000 { 5 } else { ITERATIONS };

    let mut rng = XorShift(0x6eed_2026_0815);

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
    let db_path = dir.path().join("bench.db");
    let mut db = Db::open(&db_path).expect("open bench db");

    // ------------------------------------------------------------------
    // Seed
    // ------------------------------------------------------------------
    let seed_started = Instant::now();
    let mut archived_ids = Vec::new();
    for i in 0..entries {
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
        if embed {
            let v: Vec<f32> = (0..EMBED_DIMS)
                .map(|d| ((rng.next() % 1_000_000) as f32 + d as f32) * 1e-6)
                .collect();
            let l: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            let v: Vec<f32> = v.iter().map(|x| x / l).collect();
            db.insert_embedding(id, "bench", "1", EMBED_DIMS, &v)
                .expect("embed");
        }
        // ~10% archived (ADR-0023 axis).
        if i % 10 == 0 {
            db.set_status(id, recall::domain::memory::MemoryStatus::Archived)
                .expect("archive");
            archived_ids.push(id);
        }
    }
    let seed_ms = seed_started.elapsed().as_millis();
    let query_vec: Vec<f32> = {
        let mut v: Vec<f32> = (0..EMBED_DIMS)
            .map(|d| (42.0 + d as f64 * 1e-5) as f32)
            .collect();
        let l: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter_mut().for_each(|x| *x /= l);
        v
    };
    let scoped = SearchFilter::with_project("project-03");
    let default_filter = SearchFilter::default();
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    println!("Recall Phase 6 hardening benchmark (ADR-0025)");
    println!("=============================================");
    println!("entries:      {entries} across {PROJECTS} projects (~10% archived)");
    println!(
        "embeddings:   {}",
        if embed {
            "seeded"
        } else {
            "none (FTS-only axis)"
        }
    );
    println!("seed time:    {seed_ms} ms");
    println!("db size:      {db_size} bytes");
    println!();
    println!("operation, n, avg_ms, median_ms, p95_ms, p99_ms, min_ms, max_ms");

    // ------------------------------------------------------------------
    // Search engines (database work only)
    // ------------------------------------------------------------------
    for (label, filter) in [
        ("fts, global", &default_filter),
        ("fts, scoped project-03", &scoped),
    ] {
        report(label, ITERATIONS, &mut || {
            db.search_filtered("connection pool", filter, 20).unwrap();
        });
    }
    if embed {
        for (label, filter) in [
            ("semantic, global", &default_filter),
            ("semantic, scoped project-03", &scoped),
        ] {
            report(label, ITERATIONS, &mut || {
                db.semantic_search(&query_vec, 50, "bench", "1", filter)
                    .unwrap();
            });
        }
        for (label, filter) in [
            ("hybrid, global", &default_filter),
            ("hybrid, scoped project-03", &scoped),
        ] {
            report(label, ITERATIONS, &mut || {
                db.hybrid_search("connection pool", Some(&query_vec), filter, 20)
                    .unwrap();
            });
        }
    } else {
        // Without stored embeddings the semantic side is empty; hybrid
        // collapses to FTS plus an empty vector lookup.
        report("hybrid (no embeddings), global", ITERATIONS, &mut || {
            db.hybrid_search("connection pool", Some(&query_vec), &default_filter, 20)
                .unwrap();
        });
    }
    report("hybrid, include-archived", ITERATIONS, &mut || {
        db.hybrid_search(
            "connection pool",
            Some(&query_vec),
            &SearchFilter {
                project: None,
                include_archived: true,
            },
            20,
        )
        .unwrap();
    });

    // ------------------------------------------------------------------
    // List / edit / archive / delete (library layer)
    // ------------------------------------------------------------------
    report("list 20", ITERATIONS, &mut || {
        db.list_memories_filtered(&default_filter, 20).unwrap();
    });
    let edit_target = entries as i64 / 2;
    let mut edit = db;
    report("edit", ITERATIONS, &mut || {
        recall::application::edit::run(
            &mut edit,
            &recall::cli::EditArgs {
                id: edit_target,
                solution: Some("updated solution text".into()),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let mut lifecycle_db = edit;
    report("archive", ITERATIONS, &mut || {
        lifecycle_db
            .set_status(
                archived_ids[0],
                recall::domain::memory::MemoryStatus::Archived,
            )
            .unwrap();
    });
    // Delete is timed on rows seeded for the purpose (shrinks the dataset
    // only at the very end).
    let delete_targets: Vec<i64> = (0..20)
        .map(|k| {
            lifecycle_db
                .insert_memory(
                    &NewMemory {
                        problem: format!("delete target {k}"),
                        solution: "s".into(),
                        ..Default::default()
                    },
                    OffsetDateTime::now_utc(),
                )
                .expect("delete target")
        })
        .collect();
    let mut del_iter = 0usize;
    report("delete", ITERATIONS, &mut || {
        let id = delete_targets[del_iter % delete_targets.len()];
        del_iter += 1;
        lifecycle_db.delete_memory(id).unwrap();
    });

    // ------------------------------------------------------------------
    // Export / import (application layer; heavy ops at large sizes)
    // ------------------------------------------------------------------
    let export_path = dir.path().join("export.json");
    report("export", heavy_iterations, &mut || {
        recall::application::transfer::export(&lifecycle_db, Some(&export_path), false).unwrap();
    });
    let import_iters = heavy_iterations;
    let mut import_times_ok = 0usize;
    report("import (fresh db)", import_iters, &mut || {
        let fresh_path = dir.path().join(format!("import-{import_times_ok}.db"));
        let mut fresh = Db::open(&fresh_path).expect("fresh db");
        recall::application::transfer::import(&mut fresh, &export_path, false).unwrap();
        import_times_ok += 1;
    });

    // ------------------------------------------------------------------
    // Capture breakdown (application layer, git context included)
    // ------------------------------------------------------------------
    let repo = dir.path().join("bench-repo");
    std::fs::create_dir_all(&repo).unwrap();
    for (args, status) in [
        (["init", "-b", "main"], "init"),
        (["config", "user.email", "b@example.com"], "config"),
        (["config", "user.name", "b"], "config2"),
    ] {
        let ok = std::process::Command::new("git")
            .current_dir(&repo)
            .args(args)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {status} failed");
    }
    std::fs::write(repo.join("f.txt"), "x").unwrap();
    for cmd in [&["add", "f.txt"][..], &["commit", "-m", "init"][..]] {
        assert!(
            std::process::Command::new("git")
                .current_dir(&repo)
                .args(cmd)
                .status()
                .expect("git")
                .success(),
            "git {cmd:?} failed"
        );
    }
    report("capture, git detection only", ITERATIONS, &mut || {
        recall::infrastructure::git::GitContext::detect(&repo);
        recall::infrastructure::git::detect_project(
            &repo,
            &recall::infrastructure::git::GitContext::detect(&repo),
        );
    });
    let capture_args = recall::cli::CaptureArgs {
        problem: Some("postgres pool exhaustion in benchmark".into()),
        solution: Some("raised the limit".into()),
        ..Default::default()
    };
    report("capture, dedup+insert only", ITERATIONS, &mut || {
        let memory = NewMemory {
            problem: "postgres pool exhaustion in benchmark".into(),
            solution: "raised the limit".into(),
            project: Some("bench".into()),
            ..Default::default()
        };
        lifecycle_db.find_near_identical(&memory, 30).unwrap();
        lifecycle_db
            .insert_memory(&memory, OffsetDateTime::now_utc())
            .unwrap();
    });
    report("capture, full application flow", ITERATIONS, &mut || {
        recall::application::capture::run(&mut lifecycle_db, &capture_args, &repo).unwrap();
    });

    // ------------------------------------------------------------------
    // Process startup (spawn the release binary; separate from app work)
    // ------------------------------------------------------------------
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .expect("example binary dir");
    // The example lives in target/release/examples; the recall binary is
    // one level up in target/release.
    let recall_bin = [exe_dir.join("recall.exe"), exe_dir.join("../recall.exe")]
        .into_iter()
        .find(|p| p.exists());
    if let Some(recall_bin) = recall_bin {
        let mut startup_times = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let t = Instant::now();
            let out = std::process::Command::new(&recall_bin)
                .arg("--version")
                .output()
                .expect("recall --version");
            assert!(out.status.success());
            startup_times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        startup_times.remove(0);
        startup_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg = startup_times.iter().sum::<f64>() / startup_times.len() as f64;
        println!(
            "process startup (spawn + exit), {}, {avg:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}",
            startup_times.len(),
            percentile(&startup_times, 50.0),
            percentile(&startup_times, 95.0),
            percentile(&startup_times, 99.0),
            startup_times[0],
            startup_times[startup_times.len() - 1]
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
    println!("NOTE: search rows exclude model inference and process startup;");
    println!("      model throughput: `cargo run --release --example bench_embed`.");
}
