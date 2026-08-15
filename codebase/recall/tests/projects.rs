//! Project identity, discovery, and project-aware search (ADR-0021/0022).
//!
//! Binary-level tests for the scoping UX and discovery rules, plus
//! library-level tests (synthetic vectors) for project/status filtering
//! inside the hybrid pipeline. All repositories are temporary.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use recall::domain::memory::NewMemory;
use recall::infrastructure::database::{Db, SearchFilter};
use recall::infrastructure::embeddings::EMBED_DIMS;
use time::OffsetDateTime;

use common::{stderr, stdout, temp_db_path};

const MODEL: &str = "all-MiniLM-L6-v2";
const VERSION: &str = "1";

fn run(db: &Path, args: &[&str]) -> Output {
    common::run(db, None, args, None)
}

fn capture(db: &Path, problem: &str, solution: &str, project: &str) {
    let out = run(
        db,
        &[
            "capture",
            "--problem",
            problem,
            "--solution",
            solution,
            "--project",
            project,
        ],
    );
    assert!(out.status.success(), "capture failed: {}", stderr(&out));
}

// ---------------------------------------------------------------------------
// Scoping UX
// ---------------------------------------------------------------------------

#[test]
fn project_scoped_search_excludes_other_projects() {
    let (_dir, db) = temp_db_path();
    capture(
        &db,
        "postgres pool exhaustion hit me",
        "raised pool",
        "checkout",
    );
    capture(
        &db,
        "postgres pool exhaustion hit me too",
        "pool tuning",
        "billing",
    );

    // Global default: both projects.
    let out = run(&db, &["search", "postgres pool exhaustion"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("checkout"), "{text}");
    assert!(text.contains("billing"), "{text}");

    // Scoped: exactly one project.
    let out = run(
        &db,
        &[
            "search",
            "--project",
            "checkout",
            "postgres pool exhaustion",
        ],
    );
    let text = stdout(&out);
    assert!(text.contains("checkout"), "{text}");
    assert!(
        !text.contains("billing"),
        "scoped search must exclude other projects: {text}"
    );

    // Case-insensitive matching.
    let out = run(
        &db,
        &[
            "search",
            "--project",
            "CHECKOUT",
            "postgres pool exhaustion",
        ],
    );
    assert!(stdout(&out).contains("checkout"));
}

#[test]
fn unknown_project_is_an_empty_result_not_an_error() {
    let (_dir, db) = temp_db_path();
    capture(&db, "sqlite locked", "busy timeout", "checkout");
    let out = run(
        &db,
        &["search", "--project", "does-not-exist", "sqlite locked"],
    );
    assert!(out.status.success());
    assert!(stdout(&out).contains("No results"));
}

#[test]
fn projects_command_lists_labels_counts_and_marks_current() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("recall.db");
    capture(&db, "one", "s1", "alpha");
    capture(&db, "two", "s2", "alpha");
    capture(&db, "three", "s3", "beta");

    // Run inside a directory whose name matches project "alpha".
    let work = dir.path().join("alpha");
    std::fs::create_dir_all(&work).unwrap();
    let out = common::run(&db, Some(&work), &["projects"], None);
    assert!(out.status.success(), "projects failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("beta"), "{text}");
    assert!(text.contains("2"), "alpha should have 2 memories: {text}");
    assert!(
        text.contains("*alpha"),
        "current project should be marked: {text}"
    );
}

#[test]
fn list_project_filter_scopes_the_listing() {
    let (_dir, db) = temp_db_path();
    capture(&db, "alpha problem", "s", "alpha");
    capture(&db, "beta problem", "s", "beta");
    let out = run(&db, &["list", "--project", "beta"]);
    let text = stdout(&out);
    assert!(text.contains("beta problem"), "{text}");
    assert!(!text.contains("alpha problem"), "{text}");
}

// ---------------------------------------------------------------------------
// Discovery / identity
// ---------------------------------------------------------------------------

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("git must be available");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn worktrees_share_the_project_identity() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "t@example.com"]);
    git(repo.path(), &["config", "user.name", "t"]);
    std::fs::write(repo.path().join("f.txt"), "x").unwrap();
    git(repo.path(), &["add", "f.txt"]);
    git(repo.path(), &["commit", "-m", "init"]);

    let wt_root = tempfile::tempdir().unwrap();
    let wt = wt_root.path().join("worktree");
    git(
        repo.path(),
        &["worktree", "add", wt.to_str().unwrap(), "-b", "wt-branch"],
    );

    let (_dir, db) = temp_db_path();
    for dir in [repo.path(), &wt] {
        let out = common::run(
            &db,
            Some(dir),
            &["capture", "--problem", "worktree test", "--solution", "s"],
            None,
        );
        assert!(out.status.success(), "capture failed: {}", stderr(&out));
    }
    let out = common::run(&db, Some(repo.path()), &["projects"], None);
    let text = stdout(&out);
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    assert!(
        text.contains(repo_name),
        "both worktree captures must share the repo-name identity: {text}"
    );
    // Only one project row (the two captures share one label): count 2.
    assert!(
        text.contains("2"),
        "expected 2 memories under one project: {text}"
    );
}

#[test]
fn nested_repositories_use_the_innermost_repo_name() {
    let outer = tempfile::tempdir().unwrap();
    git(outer.path(), &["init", "-b", "main"]);
    git(outer.path(), &["config", "user.email", "t@example.com"]);
    git(outer.path(), &["config", "user.name", "t"]);
    let inner = outer.path().join("inner-repo");
    std::fs::create_dir_all(&inner).unwrap();
    git(&inner, &["init", "-b", "main"]);

    let (_dir, db) = temp_db_path();
    let out = common::run(
        &db,
        Some(&inner),
        &["capture", "--problem", "nested", "--solution", "s"],
        None,
    );
    assert!(out.status.success());
    let out = common::run(&db, Some(&inner), &["projects"], None);
    assert!(
        stdout(&out).contains("inner-repo"),
        "the innermost repo decides the identity: {}",
        stdout(&out)
    );
}

// ---------------------------------------------------------------------------
// Library-level: hybrid filtering with synthetic vectors
// ---------------------------------------------------------------------------

fn open() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("recall.db")).unwrap();
    assert!(db.vec_enabled());
    (dir, db)
}

fn seed(db: &mut Db, problem: &str, project: &str, fill: f32) -> i64 {
    let id = db
        .insert_memory(
            &NewMemory {
                problem: problem.into(),
                solution: "solution".into(),
                project: Some(project.into()),
                ..Default::default()
            },
            OffsetDateTime::now_utc(),
        )
        .unwrap();
    let mut v = vec![fill; EMBED_DIMS];
    for (i, x) in v.iter_mut().enumerate() {
        *x = fill + (i as f32 * 1e-6);
    }
    let l: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter_mut().for_each(|x| *x /= l);
    db.insert_embedding(id, MODEL, VERSION, EMBED_DIMS, &v)
        .unwrap();
    id
}

#[test]
fn hybrid_project_filter_applies_to_both_engines() {
    let (_dir, mut db) = open();
    // Same problem text in two projects: FTS must obey the filter.
    seed(&mut db, "postgres connection pool exhausted", "alpha", 1.0);
    seed(&mut db, "postgres connection pool exhausted", "beta", 1.0);
    // A third memory found only semantically, in beta.
    seed(
        &mut db,
        "transactions were never released to the database",
        "beta",
        0.99,
    );
    let unit = {
        let mut v = vec![0.99f32; EMBED_DIMS];
        for (i, x) in v.iter_mut().enumerate() {
            *x = 0.99 + (i as f32 * 1e-6);
        }
        v
    };

    let global = db
        .hybrid_search(
            "database pool keeps running out of connections",
            Some(&unit),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert!(
        global.len() >= 3,
        "global search sees everything: {}",
        global.len()
    );

    let alpha = db
        .hybrid_search(
            "database pool keeps running out of connections",
            Some(&unit),
            &SearchFilter::with_project("alpha"),
            10,
        )
        .unwrap();
    assert!(
        alpha
            .iter()
            .all(|h| h.memory.project.as_deref() == Some("alpha")),
        "scoped hybrid search must return only that project"
    );
    assert_eq!(alpha.len(), 1);

    let beta = db
        .hybrid_search(
            "database pool keeps running out of connections",
            Some(&unit),
            &SearchFilter::with_project("BETA"),
            10,
        )
        .unwrap();
    assert_eq!(
        beta.len(),
        2,
        "beta has the keyword match and the semantic match"
    );
}

#[test]
fn archived_memories_are_excluded_from_hybrid_search() {
    let (_dir, mut db) = open();
    let id = seed(&mut db, "kafka consumer lag on orders", "alpha", 1.0);
    db.set_status(id, recall::domain::memory::MemoryStatus::Archived)
        .unwrap();

    let unit = vec![0.5f32; EMBED_DIMS];
    let hits = db
        .hybrid_search(
            "kafka consumer lag",
            Some(&unit),
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert!(
        hits.is_empty(),
        "archived memories must not surface by default"
    );

    let hits = db
        .hybrid_search(
            "kafka consumer lag",
            Some(&unit),
            &SearchFilter {
                project: None,
                include_archived: true,
            },
            10,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].memory.status,
        recall::domain::memory::MemoryStatus::Archived
    );
}
