//! Deterministic property tests for the high-risk boundaries.
//!
//! cargo-fuzz targets were considered and rejected: the suite stays on
//! stable Rust with deterministic pseudo-random property tests instead:
//! - no nightly toolchain requirement,
//! - no new dependencies (cargo-fuzz/proptest would enlarge the tree),
//! - fixed seeds make every failure reproducible as a regression test.
//!
//! Boundaries covered: the secret sanitizer, the FTS5 MATCH query
//! builder, and the JSON import parser. Every discovered bug must get a
//! deterministic regression test — these loops are the tripwires, not the
//! proof.

mod common;

use recall::domain::sanitize::sanitize;
use recall::infrastructure::database::fts::build_match_query;
use recall::infrastructure::database::{Db, SearchFilter};

/// Deterministic pseudo-random generator (same house pattern as the
/// benchmarks — no `rand` dependency).
struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        XorShift(seed)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Random index into a slice.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() as usize) % items.len()]
    }
}

const ROUNDS: usize = 200;

// ---------------------------------------------------------------------------
// Sanitizer
// ---------------------------------------------------------------------------

#[test]
fn sanitizer_never_leaks_embedded_secrets() {
    // Embed a known secret shape in random surrounding text: the secret's
    // distinctive fragment must never survive sanitization.
    let secrets: &[(&str, &str)] = &[
        ("--password hunter2", "hunter2"),
        ("--token=abc123secret", "abc123secret"),
        ("Bearer eyJabc.def.ghi", "eyJabc.def.ghi"),
        ("ghp_abcdefghijklmnop", "ghp_abcdefghijklmnop"),
        ("xoxb-123456789012-abcdefghijklmnop", "xoxb-123456789012"),
        ("sk_live_51abcdefghijklmnop", "sk_live_51abcdefghijklmnop"),
        ("AKIAIOSFODNN7EXAMPLE", "AKIAIOSFODNN7EXAMPLE"),
        ("https://alice:hunter2@host/x", "hunter2"),
        ("DB_PASSWORD=supersecret", "supersecret"),
        ("Authorization: Bearer abc.def", "abc.def"),
    ];
    const NOISE: &[char] = &[
        'a', 'b', 'c', ' ', 'x', 'y', 'z', ' ', '1', '2', '3', ' ', '\n', '"', '\'', ',', ';', '(',
        ')', ':', '.', '!', '?',
    ];
    let mut rng = XorShift::new(0x5eed_0001);
    for round in 0..ROUNDS {
        let (shape, fragment) = *rng.pick(secrets);
        let prefix_len = (rng.next() as usize) % 20;
        let suffix_len = (rng.next() as usize) % 20;
        let mut text = String::new();
        for _ in 0..prefix_len {
            text.push(*rng.pick(NOISE));
        }
        // A guaranteed boundary before the shape so the boundary-before
        // rules (flags, keys, Bearer, tokens) can fire.
        text.push(' ');
        text.push_str(shape);
        for _ in 0..suffix_len {
            text.push(*rng.pick(NOISE));
        }
        let report = sanitize(&text);
        assert!(
            report.redactions >= 1,
            "round {round}: secret shape '{shape}' must be redacted from: {text:?}"
        );
        assert!(
            !report.sanitized.contains(fragment),
            "round {round}: fragment '{fragment}' leaked through: {}",
            report.sanitized
        );
    }
}

#[test]
fn sanitizer_passes_clean_text_through_untouched() {
    // Random text without any secret markers must come back unchanged.
    // The charset is deliberately free of the pattern starters: no `-`,
    // `=`, `:`, `_`, uppercase (so no `eyJ`, `AKIA`, `xoxb-`, …), and no
    // `b` (so the word `bearer` cannot form). No rule can fire.
    const CLEAN: &[char] = &[
        'a', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
        't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ' ',
        '\n', '\t', '.', ',',
    ];
    let mut rng = XorShift::new(0x5eed_0002);
    for _ in 0..ROUNDS {
        let len = (rng.next() as usize) % 300;
        let mut text = String::new();
        for _ in 0..len {
            text.push(*rng.pick(CLEAN));
        }
        let report = sanitize(&text);
        assert_eq!(
            report.redactions, 0,
            "clean text must not be redacted: {text:?} -> {}",
            report.sanitized
        );
        assert_eq!(report.sanitized, text);
    }
}

// ---------------------------------------------------------------------------
// FTS5 MATCH query builder
// ---------------------------------------------------------------------------

#[test]
fn fts_query_builder_never_produces_a_rejected_match_expression() {
    // Whatever garbage goes in, the built MATCH expression must be
    // executable against the real FTS5 table — no SQL injection, no
    // unbalanced quotes, no parser errors from SQLite.
    const NASTY: &[char] = &[
        '"', '\'', '*', '(', ')', ':', '-', '_', '.', ',', ';', '&', '|', '<', '>', '=', '!', '?',
        '[', ']', '{', '}', '^', '~', '`', '$', '%', '#', '@', '+', 'a', 'B', 'z', '0', '9', ' ',
        '\t', '\n', 'é', 'ß', 'Ω', '中',
    ];
    let mut rng = XorShift::new(0x5eed_0003);
    let (_dir, db_path) = common::temp_db_path();
    {
        let mut db = Db::open(&db_path).unwrap();
        db.insert_memory(
            &recall::domain::memory::NewMemory {
                problem: "seed entry for the property table".into(),
                solution: "s".into(),
                ..Default::default()
            },
            time::OffsetDateTime::now_utc(),
        )
        .unwrap();
    }
    let db = Db::open(&db_path).unwrap();
    for round in 0..ROUNDS {
        let len = (rng.next() as usize) % 40;
        let mut query = String::new();
        for _ in 0..len {
            query.push(*rng.pick(NASTY));
        }
        match build_match_query(&query) {
            Err(e) => {
                // The only rejection: no searchable word at all.
                let has_word = query
                    .split_whitespace()
                    .any(|t| t.chars().any(|c| c.is_alphanumeric()));
                assert!(
                    !has_word,
                    "round {round}: query with words was rejected: {query:?} -> {e}"
                );
            }
            Ok(match_expr) => {
                // Execute the built expression directly against FTS5 —
                // SQLite must accept whatever the builder produced.
                let outcome = db.with_connection(|c| {
                    c.query_row(
                        "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH ?1",
                        [&match_expr],
                        |r| r.get::<_, i64>(0),
                    )
                });
                assert!(
                    outcome.is_ok(),
                    "round {round}: SQLite rejected the built MATCH expression \
                     {match_expr:?} for input {query:?}: {}",
                    outcome.unwrap_err()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// JSON import parser
// ---------------------------------------------------------------------------

#[test]
fn import_parser_never_panics_and_never_partially_writes() {
    // Random byte soup (including malformed JSON, truncated exports, and
    // valid-looking envelopes) must produce either a clean success or a
    // clean error — never a panic, and on error the database must be
    // unchanged.
    const SOUP: &[char] = &[
        '{', '}', '[', ']', '"', '\\', ':', ',', 'a', 'b', 'c', '0', '1', '2', '.', ' ', '\n',
        '\t', '-', '_', '/', '\'', '=', '\u{00e9}', '\u{00df}', '\u{4e2d}', '\u{0000}', '\u{fffd}',
    ];
    let mut rng = XorShift::new(0x5eed_0004);
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("recall.db");
    let import_path = dir.path().join("import.json");

    let mut db = Db::open(&db_path).unwrap();
    db.insert_memory(
        &recall::domain::memory::NewMemory {
            problem: "pre-existing memory".into(),
            solution: "s".into(),
            ..Default::default()
        },
        time::OffsetDateTime::now_utc(),
    )
    .unwrap();

    // Occasionally splice a structurally valid envelope around the soup.
    let envelope = r#"{"format_version":1,"exported_at":"2026-08-15T00:00:00.000Z","recall_schema_version":3,"memories":[]}"#;

    for round in 0..ROUNDS {
        let len = (rng.next() as usize) % 200;
        let mut content = String::new();
        for _ in 0..len {
            content.push(*rng.pick(SOUP));
        }
        // Every third round starts from the valid envelope and mutates it,
        // so imports reach deeper parsing stages too.
        if round % 3 == 0 {
            let mut mutated = envelope.to_string();
            for _ in 0..((rng.next() as usize) % 20) {
                // Char boundaries shift after every replacement, so the
                // index list is rebuilt each iteration.
                let indices: Vec<usize> = mutated.char_indices().map(|(i, _)| i).collect();
                if indices.is_empty() {
                    break;
                }
                let pos = indices[(rng.next() as usize) % indices.len()];
                let replacement = *rng.pick(SOUP);
                mutated.remove(pos);
                mutated.insert(pos, replacement);
            }
            content = mutated;
        }
        std::fs::write(&import_path, &content).unwrap();

        let before: i64 = db
            .with_connection(|c| c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0)))
            .unwrap();
        // The import must never panic (a panic would abort the test).
        let result = recall::application::transfer::import(&mut db, &import_path, false);
        let after: i64 = db
            .with_connection(|c| c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0)))
            .unwrap();
        if result.is_err() {
            assert_eq!(
                before, after,
                "round {round}: a failed import must not change the database"
            );
        }
    }

    // The store is still fully usable after all that abuse.
    let hits = db
        .search_filtered("pre-existing", &SearchFilter::default(), 10)
        .unwrap();
    assert_eq!(hits.len(), 1);
}

/// `import` on a valid file with zero memories is a clean no-op success.
#[test]
fn import_of_an_empty_export_is_a_clean_success() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("recall.db");
    let import_path = dir.path().join("empty.json");
    std::fs::write(
        &import_path,
        r#"{"format_version":1,"exported_at":"2026-08-15T00:00:00.000Z","recall_schema_version":3,"memories":[]}"#,
    )
    .unwrap();
    let mut db = Db::open(&db_path).unwrap();
    recall::application::transfer::import(&mut db, &import_path, false).unwrap();
    let count: i64 = db
        .with_connection(|c| c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(count, 0);
}
