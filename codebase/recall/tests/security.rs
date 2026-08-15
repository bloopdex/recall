//! Security guardrails.
//!
//! Recall is local-first with a zero-network mandate (ADR-0000/0010).
//! The strongest enforceable guard at this layer is the dependency
//! surface: no networking crate may enter the tree. The runtime guarantee
//! ("never upload data") follows from that — the binary has no code path
//! that can reach the network.

/// Networking / async-runtime crate names that must never appear as
/// direct dependencies. (Transitive C libraries like the bundled SQLite
/// are compile-time only and have no network surface.)
const BANNED_DEPS: &[&str] = &[
    "reqwest",
    "hyper",
    "tokio",
    "ureq",
    "isahc",
    "curl",
    "openssl",
    "rustls",
    "native-tls",
    "surf",
    "attohttpc",
    "minreq",
    "websocket",
    "async-std",
    "smol",
];

#[test]
fn cargo_manifest_contains_no_network_dependencies() {
    // Tests run with the crate root as the working directory.
    let manifest = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml readable");
    let lower = manifest.to_lowercase();
    for banned in BANNED_DEPS {
        // Match on the dependency name, not on random words in comments.
        let needle = format!("{banned} =");
        if let Some(pos) = lower.find(&needle) {
            let line = lower[pos..].lines().next().unwrap_or("");
            // ADR-0013 carve-out: exactly one network crate is allowed —
            // reqwest — and only behind the opt-in `download` feature
            // (optional = true), used exclusively by
            // `recall embeddings download`. Everything else stays banned.
            assert!(
                *banned == "reqwest" && line.contains("optional = true"),
                "banned dependency '{banned}' found in Cargo.toml — only reqwest is allowed, and only behind the opt-in `download` feature"
            );
        }
    }
    // The feature that gates the network crate must exist and stay
    // default-off.
    assert!(
        lower.contains("download = [\"dep:reqwest\"]"),
        "the `download` feature must gate the network crate"
    );
    assert!(
        lower.contains("default = []"),
        "the default feature set must stay network-free"
    );
}

/// Source-level zero-network guard: Recall's own modules must never
/// reference network-crate APIs. The only sanctioned exception is
/// `infrastructure/embeddings/download.rs`, which uses reqwest behind the
/// opt-in `download` feature (ADR-0010 amendment / ADR-0013).
#[test]
fn recall_source_has_no_network_api_references() {
    fn collect(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("src readable") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    collect(std::path::Path::new("src"), &mut files);
    for file in files {
        let is_download_module = file.to_string_lossy().ends_with("embeddings\\download.rs")
            || file.to_string_lossy().ends_with("embeddings/download.rs");
        if is_download_module {
            continue;
        }
        let source = std::fs::read_to_string(&file).unwrap();
        for needle in ["reqwest", "hyper::", "tokio::", "ureq::", "ureq ", "::ureq"] {
            assert!(
                !source.contains(needle),
                "network API reference '{needle}' found in {} — Recall's own code must have no network paths",
                file.display()
            );
        }
    }
}

/// Documented safeguard: capture never auto-collects environment variables
/// or secret-bearing process state. The only fields persisted are the ones
/// the user typed plus fixed git metadata (branch, commit SHA, changed-file
/// names). There is no code path that reads `std::env::vars()`.
#[test]
fn capture_does_not_auto_collect_environment_variables() {
    // Enforced by construction: the capture path (application::capture)
    // reads no process environment. This test pins that the only env-based
    // configuration is the database path, which is stored nowhere.
    let source = std::fs::read_to_string("src/application/capture.rs").unwrap();
    assert!(
        !source.contains("env::vars"),
        "capture must not read environment variables"
    );
    assert!(
        !source.contains("var_os"),
        "capture must not read environment variables"
    );
}

/// Full transitive-tree scan (Phase 6): the manifest check above only sees
/// direct dependencies. This test scans the whole `cargo tree`, so a
/// network crate can never enter behind a renamed or indirect dependency
/// either. Sanctioned paths (ADR-0010 amendment 2 / ADR-0013):
/// - default build: only under `fastembed` (hf-hub's downloader — a
///   code path Recall's runtime never invokes);
/// - `download` feature: the opt-in reqwest stack.
///
/// This is the same rule as `scripts/check_no_network.ps1`, running
/// automatically on every platform in `cargo test` (and therefore CI).
#[test]
fn full_dependency_tree_contains_no_network_crates() {
    fn run_tree(features: Option<&str>) -> String {
        let mut cmd = std::process::Command::new("cargo");
        cmd.args(["tree", "--edges", "normal", "--prefix", "none"]);
        if let Some(f) = features {
            cmd.args(["--features", f]);
        }
        let out = cmd
            .output()
            .expect("cargo must be available (tests run under cargo)");
        assert!(
            out.status.success(),
            "cargo tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }
    fn parent_chain(name: &str, features: Option<&str>) -> String {
        let mut cmd = std::process::Command::new("cargo");
        cmd.args(["tree", "-i", name, "--edges", "normal", "--prefix", "none"]);
        if let Some(f) = features {
            cmd.args(["--features", f]);
        }
        let out = cmd
            .output()
            .expect("cargo must be available (tests run under cargo)");
        assert!(
            out.status.success(),
            "cargo tree -i {name} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    // Default build. (reqwest itself may appear under fastembed — hf-hub
    // pulls it in — which is exactly the fastembed carve-out; Recall's own
    // optional direct dependency is pinned by the manifest check above.)
    let default_tree = run_tree(None);
    for line in default_tree.lines() {
        let name = line.split_whitespace().next().unwrap_or("");
        if !BANNED_DEPS.contains(&name) {
            continue;
        }
        let chain = parent_chain(name, None);
        assert!(
            chain.contains("fastembed"),
            "banned network crate '{name}' found in the default dependency tree outside the fastembed carve-out:\n{chain}"
        );
    }

    // Opt-in download feature: the reqwest stack becomes sanctioned.
    let download_tree = run_tree(Some("download"));
    for line in download_tree.lines() {
        let name = line.split_whitespace().next().unwrap_or("");
        if !BANNED_DEPS.contains(&name) {
            continue;
        }
        let chain = parent_chain(name, Some("download"));
        assert!(
            chain.contains("fastembed") || chain.contains("reqwest"),
            "banned network crate '{name}' in the download tree is not reachable only via fastembed/reqwest:\n{chain}"
        );
    }
}

/// ADR-0010 amendment (Phase 4): the ONLY module allowed to read
/// environment variables is `infrastructure/shell.rs`, and it may read
/// exactly the whitelist below — the three snapshot variables written by
/// Recall's own prompt hook, plus the home/shell-location variables needed
/// to find startup files. It never enumerates the environment, and nothing
/// outside the whitelist can enter a memory.
#[test]
fn shell_integration_reads_only_whitelisted_environment_variables() {
    const ALLOWED: &[&str] = &[
        "RECALL_LAST_COMMAND",
        "RECALL_LAST_EXIT_CODE",
        "RECALL_LAST_CWD",
        "HOME",
        "USERPROFILE",
        "SHELL",
    ];
    let source = std::fs::read_to_string("src/infrastructure/shell.rs").unwrap();
    for line in source.lines() {
        let Some(pos) = line.find("env::var(\"") else {
            continue;
        };
        let rest = &line[pos + "env::var(\"".len()..];
        let name = rest.split('"').next().unwrap_or("");
        assert!(
            ALLOWED.contains(&name),
            "shell.rs reads environment variable '{name}' outside the Phase 4 whitelist"
        );
    }
}
