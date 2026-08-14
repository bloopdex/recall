//! Capture wall-clock baseline (Phase 2 target: <100ms per capture with
//! git context — baseline first, per the BloopLab performance standard).
//!
//! Run:  cargo run --release --example bench_capture
//!
//! Measures the real capture operation (application layer): input
//! resolution, git/project detection against a real temp git repo,
//! near-identical check, and the transactional insert — NOT just raw
//! insert throughput. For the end-to-end spawned-binary numbers (including
//! process startup), see tests/bench_capture.rs.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use recall::application;
use recall::cli::CaptureArgs;
use recall::infrastructure::database::Db;

const ITERATIONS: usize = 50;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("git must be available");
    assert!(status.success(), "git {args:?} failed");
}

fn main() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("bench-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "bench@example.com"]);
    git(&repo, &["config", "user.name", "bench"]);
    std::fs::write(repo.join("src.txt"), "content").unwrap();
    git(&repo, &["add", "src.txt"]);
    git(&repo, &["commit", "-m", "init"]);
    std::fs::write(repo.join("dirty.txt"), "uncommitted").unwrap();

    let mut db = Db::open(&dir.path().join("recall.db")).expect("open db");

    let args = CaptureArgs {
        problem: Some("postgres connection pool exhausted on checkout-service".into()),
        solution: Some("raised max_connections and enabled pgbouncer".into()),
        error: Some("too many clients".into()),
        project: None, // exercise automatic detection
        ..Default::default()
    };

    // Warm-up (includes connection/migration work already done by open).
    application::capture::run(&mut db, &args, &repo).expect("warm-up capture");

    let mut times = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let started = Instant::now();
        application::capture::run(&mut db, &args, &repo).expect("capture");
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = times.iter().sum::<f64>() / times.len() as f64;

    println!("Recall capture baseline (in-process operation)");
    println!("=============================================");
    println!(
        "build:       {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!("iterations:  {ITERATIONS}");
    println!("avg:         {avg:.2} ms");
    println!("min:         {:.2} ms", times[0]);
    println!("max:         {:.2} ms", times[times.len() - 1]);
    println!("median:      {:.2} ms", times[times.len() / 2]);
    println!("workload:    capture with git detection (branch + commit + dirty file)");
    println!(
        "             + dedup check against {} stored entries",
        ITERATIONS + 1
    );
    println!(
        "machine:     {} / {} logical cpus",
        std::env::consts::OS,
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
    println!();
    println!("NOTE: this measures the capture operation in-process. End-to-end");
    println!("CLI numbers (including process startup) live in tests/bench_capture.rs.");
}
