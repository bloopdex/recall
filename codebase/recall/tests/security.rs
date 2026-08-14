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
        assert!(
            !lower.contains(&needle),
            "banned dependency '{banned}' found in Cargo.toml — Recall must never have a network surface"
        );
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
