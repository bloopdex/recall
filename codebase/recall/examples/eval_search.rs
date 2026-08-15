//! Search-quality evaluation harness (Phase 3 DoD).
//!
//! Run:  cargo run --release --example eval_search
//!
//! A deterministic corpus of 24 realistic Recall memories across eight
//! categories, with 8 queries and relevance judgments. Reports Recall@5,
//! Precision@5 and MRR for FTS-only vs hybrid search — no quality claim
//! without measurement (ADR-0016). Requires the local model; exits with a
//! message when it is missing.

use time::OffsetDateTime;

use recall::infrastructure::database::{Db, SearchFilter};
use recall::infrastructure::embeddings::{Embedder, MODEL_ID, MODEL_VERSION};

/// (problem, error, category)
const CORPUS: &[(&str, &str, &str)] = &[
    // database (ids 1-3)
    (
        "Postgres connections were exhausted because transactions weren't being released.",
        "connection pool exhausted: too many clients",
        "database",
    ),
    (
        "sqlite database is locked during migration",
        "database is locked (code 5)",
        "database",
    ),
    (
        "queries became slow after we added the new index",
        "Seq Scan on large_table",
        "database",
    ),
    // docker (4-6)
    (
        "container keeps restarting right after startup",
        "exited with code 1",
        "docker",
    ),
    (
        "image build fails at the apt-get step",
        "404 Not Found [IP: 1.2.3.4 80]",
        "docker",
    ),
    (
        "docker compose volumes are not shared between services",
        "no such file or directory",
        "docker",
    ),
    // git (7-9)
    (
        "accidentally committed secrets into the repository history",
        "failed to push some refs",
        "git",
    ),
    (
        "merge conflicts keep reappearing on the release branch",
        "CONFLICT (content)",
        "git",
    ),
    (
        "git operations hang when the remote is unreachable",
        "could not resolve host",
        "git",
    ),
    // dependencies (10-12)
    (
        "npm install fails with a checksum mismatch",
        "integrity checksum failed",
        "deps",
    ),
    (
        "dependency upgrade broke the whole build on Monday",
        "conflicting peer dependencies",
        "deps",
    ),
    (
        "lockfile changes churn every single PR",
        "package-lock.json modified",
        "deps",
    ),
    // networking (13-15)
    (
        "calls to the payment API time out after the load balancer change",
        "TLS handshake timeout",
        "network",
    ),
    (
        "DNS resolution randomly fails inside the cluster",
        "NXDOMAIN",
        "network",
    ),
    (
        "websocket connections drop every 60 seconds",
        "connection reset by peer",
        "network",
    ),
    // configuration (16-18)
    (
        "feature flag flipped differently between staging and production",
        "missing flag value",
        "config",
    ),
    (
        "environment variables differ between local and CI",
        "undefined variable",
        "config",
    ),
    (
        "the service picks up the wrong config file at boot",
        "no config file found",
        "config",
    ),
    // testing (19-21)
    (
        "tests pass locally but fail in CI",
        "expected true to be false",
        "testing",
    ),
    (
        "flaky test fails roughly every tenth run",
        "timed out after 5000ms",
        "testing",
    ),
    (
        "test suite takes twenty minutes to finish",
        "jest exited",
        "testing",
    ),
    // deployment (22-24)
    (
        "deploy succeeds but the new version never receives traffic",
        "health check failed",
        "deploy",
    ),
    (
        "rolling deploy breaks requests during the switchover window",
        "502 Bad Gateway",
        "deploy",
    ),
    (
        "rollback left the database and the app out of sync",
        "unknown column",
        "deploy",
    ),
];

/// (query, expected relevant corpus indexes)
/// KEYWORD segment: exact-ish queries FTS should nail (regression check).
/// PARAPHRASE segment: zero keyword overlap — the semantic-gap case.
const KEYWORD_QUERIES: &[(&str, &[usize])] = &[
    ("connection pool exhausted", &[0]),
    ("sqlite database is locked", &[1]),
    ("npm install fails", &[9]),
    ("tls handshake timeout", &[12]),
];

const PARAPHRASE_QUERIES: &[(&str, &[usize])] = &[
    ("database pool keeps running out of connections", &[0]), // paraphrase of 0
    ("sqlite file is busy and write fails", &[1]),            // paraphrase of 1
    ("docker container crashes on boot", &[3]),               // paraphrase of 3
    ("npm dependency checksum error", &[9]),                  // paraphrase of 9
    ("api requests time out", &[12]),                         // paraphrase of 12
    ("config drift between environments", &[15]),             // paraphrase of 15
    ("tests green locally but red on the pipeline", &[18]),   // paraphrase of 18
    ("zero-downtime rollout problems", &[22]),                // paraphrase of 22
];

fn recall_at_k(hits: &[usize], expected: &[usize], k: usize) -> f64 {
    let top: Vec<usize> = hits.iter().take(k).copied().collect();
    expected.iter().filter(|e| top.contains(e)).count() as f64 / expected.len() as f64
}

fn precision_at_k(hits: &[usize], expected: &[usize], k: usize) -> f64 {
    let top: Vec<usize> = hits.iter().take(k).copied().collect();
    if top.is_empty() {
        return 0.0;
    }
    top.iter().filter(|h| expected.contains(h)).count() as f64 / top.len() as f64
}

fn mrr(hits: &[usize], expected: &[usize]) -> f64 {
    for (i, h) in hits.iter().enumerate() {
        if expected.contains(h) {
            return 1.0 / (i as f64 + 1.0);
        }
    }
    0.0
}

fn main() {
    let embedder = match Embedder::try_load() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("model not available ({e}) — run `recall embeddings download` first");
            std::process::exit(2);
        }
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Db::open(&dir.path().join("eval.db")).expect("open db");
    let t0 = OffsetDateTime::now_utc();

    let mut ids = Vec::new();
    for (i, (problem, error, _category)) in CORPUS.iter().enumerate() {
        let memory = recall::domain::memory::NewMemory {
            problem: problem.to_string(),
            solution: format!("solution for {i}"),
            error: Some(error.to_string()),
            project: Some(format!("corpus-{}", i / 3)),
            ..Default::default()
        };
        let id = db
            .insert_memory(&memory, t0 + time::Duration::minutes(i as i64))
            .unwrap();
        let text = recall::infrastructure::embeddings::embedded_text(problem, Some(error), None);
        let vector = embedder.embed_one(&text).unwrap();
        db.insert_embedding(id, MODEL_ID, MODEL_VERSION, vector.len(), &vector)
            .unwrap();
        ids.push(id as usize);
    }

    println!(
        "Recall search-quality evaluation (corpus: {} memories, {} queries)",
        CORPUS.len(),
        KEYWORD_QUERIES.len() + PARAPHRASE_QUERIES.len()
    );
    println!("query | fts R@5 | fts P@5 | fts MRR | hybrid R@5 | hybrid P@5 | hybrid MRR");

    for (label, queries) in [
        ("KEYWORD", KEYWORD_QUERIES),
        ("PARAPHRASE", PARAPHRASE_QUERIES),
    ] {
        println!("-- {label} --");
        let mut totals = [0.0f64; 6];
        for (query, expected) in queries {
            // Judgments are corpus indexes; hits are DB ids (index + 1).
            let expected_ids: Vec<usize> = expected.iter().map(|i| ids[*i]).collect();
            let fts_hits: Vec<usize> = db
                .search(query, 5)
                .unwrap()
                .iter()
                .map(|h| h.memory.id as usize)
                .collect();
            let qvec = embedder.embed_one(query).unwrap();
            let hybrid_hits: Vec<usize> = db
                .hybrid_search(query, Some(&qvec), &SearchFilter::default(), 5)
                .unwrap()
                .iter()
                .map(|h| h.memory.id as usize)
                .collect();

            let row = [
                recall_at_k(&fts_hits, &expected_ids, 5),
                precision_at_k(&fts_hits, &expected_ids, 5),
                mrr(&fts_hits, &expected_ids),
                recall_at_k(&hybrid_hits, &expected_ids, 5),
                precision_at_k(&hybrid_hits, &expected_ids, 5),
                mrr(&hybrid_hits, &expected_ids),
            ];
            for (i, v) in row.iter().enumerate() {
                totals[i] += v;
            }
            println!(
                "\"{query}\" | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2}",
                row[0], row[1], row[2], row[3], row[4], row[5]
            );
        }
        let n = queries.len() as f64;
        println!(
            "  avg fts:    Recall@5 {:.2}  Precision@5 {:.2}  MRR {:.2}",
            totals[0] / n,
            totals[1] / n,
            totals[2] / n
        );
        println!(
            "  avg hybrid: Recall@5 {:.2}  Precision@5 {:.2}  MRR {:.2}",
            totals[3] / n,
            totals[4] / n,
            totals[5] / n
        );
    }
}
