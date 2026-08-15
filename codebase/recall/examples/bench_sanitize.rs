//! Sanitization throughput baseline (ADR-0018): how long the secret
//! scanner takes over realistic auto-captured text.
//!
//! Run: cargo run --release --example bench_sanitize

use recall::domain::sanitize::sanitize;

fn main() {
    // A 10 KB error log mixing normal output with several secret shapes.
    let mut log = String::new();
    for i in 0..50 {
        log.push_str(&format!(
            "2026-08-15T00:00:00Z INFO worker[{i}] processing batch {i}\n"
        ));
        if i % 10 == 0 {
            log.push_str("client connected https://alice:hunter2@db.internal:5432/app\n");
        }
        if i % 15 == 0 {
            log.push_str("authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abc.def\n");
        }
        if i % 25 == 0 {
            log.push_str("export DB_PASSWORD=super-secret-value\n");
        }
    }
    log.push_str("worker[3] uploaded AKIAIOSFODNN7EXAMPLE to the queue\n");
    let kb = log.len() as f64 / 1024.0;

    let iterations = 500;
    let start = std::time::Instant::now();
    let mut total_redactions = 0usize;
    for _ in 0..iterations {
        total_redactions += sanitize(&log).redactions;
    }
    let elapsed = start.elapsed();

    println!("text size: {kb:.1} KB, {iterations} iterations");
    println!(
        "total: {:.2} ms -> {:.2} us/scan, {:.1} MB/s",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64,
        (kb * iterations as f64) / 1024.0 / elapsed.as_secs_f64()
    );
    println!("redactions per scan: {}", total_redactions / iterations);
}
