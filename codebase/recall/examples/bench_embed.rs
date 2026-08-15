//! Embedding generation baseline.
//!
//! Run:  cargo run --release --example bench_embed
//!
//! Measures model load time, single-text embedding latency, and batch
//! throughput for the local all-MiniLM-L6-v2 model. Requires the model.

use std::time::Instant;

use recall::infrastructure::embeddings::Embedder;

const ITERATIONS: usize = 50;
const BATCH: usize = 32;

fn main() {
    let started = Instant::now();
    let embedder = match Embedder::try_load() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("model not available ({e})");
            std::process::exit(2);
        }
    };
    let load_ms = started.elapsed().as_millis();

    let text = "Postgres connections were exhausted because transactions weren't being released.";
    embedder.embed_one(text).expect("warm-up");

    let mut single = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let t = Instant::now();
        embedder.embed_one(text).expect("embed");
        single.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    single.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let batch_texts: Vec<&str> = vec![
        "sqlite database is locked",
        "connection pool exhausted",
        "TLS handshake timeout",
        "kafka consumer lag",
    ]
    .into_iter()
    .cycle()
    .take(BATCH)
    .collect();
    embedder.embed(&batch_texts).expect("batch warm-up");
    let mut batch_times = Vec::with_capacity(20);
    for _ in 0..20 {
        let t = Instant::now();
        embedder.embed(&batch_texts).expect("batch");
        batch_times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    batch_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("Recall embedding baseline (all-MiniLM-L6-v2, fp32 ONNX, CPU)");
    println!("==========================================================");
    println!(
        "build:       {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!("model load:  {load_ms} ms (one-time per CLI invocation)");
    println!(
        "single text ({ITERATIONS} iters): avg {:.2} ms | min {:.2} | max {:.2}",
        single.iter().sum::<f64>() / ITERATIONS as f64,
        single[0],
        single[ITERATIONS - 1]
    );
    println!(
        "batch {BATCH} (20 iters):     avg {:.2} ms | min {:.2} | max {:.2}",
        batch_times.iter().sum::<f64>() / 20.0,
        batch_times[0],
        batch_times[19]
    );
    println!(
        "machine:     {} / {} logical cpus",
        std::env::consts::OS,
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
}
