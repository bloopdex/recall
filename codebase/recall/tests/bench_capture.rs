//! End-to-end capture wall-clock benchmark: spawns the real `recall`
//! binary (process startup + CLI + git detection + insert) against a temp
//! git repo. Ignored by default — run explicitly:
//!
//!   cargo test --release --test bench_capture -- --ignored --nocapture

#![allow(dead_code)]

mod common;

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use common::temp_db_path;

const ITERATIONS: usize = 30;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("git must be available");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
#[ignore = "manual benchmark: cargo test --release --test bench_capture -- --ignored --nocapture"]
fn spawned_binary_capture_latency() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("bench-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "bench@example.com"]);
    git(&repo, &["config", "user.name", "bench"]);
    std::fs::write(repo.join("src.txt"), "x").unwrap();
    git(&repo, &["add", "src.txt"]);
    git(&repo, &["commit", "-m", "init"]);

    let (_db_dir, db) = temp_db_path();

    // Warm-up: migrations + first capture.
    let warm = Command::new(common::bin())
        .arg("--db")
        .arg(&db)
        .args(["capture", "--problem", "warm-up", "--solution", "warm"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(warm.status.success());

    let mut times = Vec::with_capacity(ITERATIONS);
    for i in 0..ITERATIONS {
        let started = Instant::now();
        let out = Command::new(common::bin())
            .arg("--db")
            .arg(&db)
            .args([
                "capture",
                "--problem",
                &format!("bench capture {i}"),
                "--solution",
                "measure",
            ])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success());
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = times.iter().sum::<f64>() / times.len() as f64;

    println!("recall spawned-binary capture latency");
    println!(
        "build: {}  iterations: {ITERATIONS}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!(
        "avg {avg:.2} ms | min {:.2} ms | max {:.2} ms | median {:.2} ms",
        times[0],
        times[times.len() - 1],
        times[times.len() / 2]
    );
}
