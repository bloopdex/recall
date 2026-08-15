//! Install-script behavior (Phase 7, ADR-0031).
//!
//! The install scripts are minimal by design: copy the binary into a
//! user bin directory, verify checksums when present, print PATH
//! guidance, and NEVER touch shell profiles, PATH, the database, or the
//! integrations. The Windows tests run the real install.ps1 against a
//! fake release directory (the compiled test binary plays the role of
//! recall.exe) and pin: placement, idempotency, checksum verification,
//! and tamper refusal. install.sh is exercised through `sh` when
//! available (gracefully skipped otherwise).

mod common;

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is two levels under the repo root")
        .to_path_buf()
}

#[cfg(windows)]
fn sha256_file(path: &Path) -> String {
    // certutil is present on every Windows install; no crypto hand-rolled
    // here and no fragile inline -Command quoting.
    let out = std::process::Command::new("certutil")
        .args(["-hashfile", path.to_str().unwrap(), "SHA256"])
        .output()
        .expect("certutil must run");
    assert!(
        out.status.success(),
        "certutil failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .nth(1)
        .expect("certutil prints the hash on the second line")
        .trim()
        .to_string()
}

/// Build a fake release directory: a copy of the test binary named
/// recall.exe plus a SHA256SUMS entry for it.
#[cfg(windows)]
fn fake_release_dir(dir: &Path) -> PathBuf {
    let release = dir.join("release");
    std::fs::create_dir_all(&release).unwrap();
    let binary = release.join("recall.exe");
    std::fs::copy(common::bin(), &binary).expect("copy test binary");
    let hash = sha256_file(&binary);
    std::fs::write(release.join("SHA256SUMS"), format!("{hash}  recall.exe\n")).unwrap();
    release
}

#[cfg(windows)]
#[test]
fn install_ps1_places_the_binary_verifies_checksums_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let release = fake_release_dir(dir.path());
    let bin_dir = dir.path().join("bin");

    let script = repo_root().join("scripts/install.ps1");
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            script.to_str().unwrap(),
            "-From",
            release.to_str().unwrap(),
            "-BinDir",
            bin_dir.to_str().unwrap(),
        ])
        .output()
        .expect("powershell must run");
    assert!(
        out.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Checksum verified"), "{stdout}");
    assert!(stdout.contains("Installed:"), "{stdout}");
    assert!(bin_dir.join("recall.exe").exists(), "binary must be placed");

    // Idempotent: a second install succeeds.
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            script.to_str().unwrap(),
            "-From",
            release.to_str().unwrap(),
            "-BinDir",
            bin_dir.to_str().unwrap(),
        ])
        .output()
        .expect("powershell must run");
    assert!(
        out.status.success(),
        "second install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(windows)]
#[test]
fn install_ps1_refuses_a_tampered_binary() {
    let dir = tempfile::tempdir().unwrap();
    let release = fake_release_dir(dir.path());
    // Tamper with the release binary (flip the last byte).
    let binary = release.join("recall.exe");
    let mut bytes = std::fs::read(&binary).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&binary, &bytes).unwrap();

    let bin_dir = dir.path().join("bin");
    let script = repo_root().join("scripts/install.ps1");
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            script.to_str().unwrap(),
            "-From",
            release.to_str().unwrap(),
            "-BinDir",
            bin_dir.to_str().unwrap(),
        ])
        .output()
        .expect("powershell must run");
    assert!(
        !out.status.success(),
        "a checksum mismatch must refuse the install"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Checksum mismatch"), "{stderr}");
    assert!(
        !bin_dir.join("recall.exe").exists(),
        "nothing may be installed from a tampered binary"
    );
}

#[test]
fn install_sh_places_the_binary_when_sh_is_available() {
    let sh_ok = std::process::Command::new("sh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !sh_ok {
        eprintln!("skipping: `sh` not available in this environment");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // install.sh expects a `recall` binary next to SHA256SUMS.
    let release = dir.path().join("release");
    std::fs::create_dir_all(&release).unwrap();
    let binary = release.join("recall");
    std::fs::copy(common::bin(), &binary).expect("copy test binary");
    // sha256sum lives next to sh on every platform that has sh here.
    let hash = {
        let out = std::process::Command::new("sha256sum")
            .arg(binary.to_str().unwrap())
            .output()
            .expect("sha256sum must run alongside sh");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(hash.len(), 64, "sha256sum must produce a hash");
    std::fs::write(release.join("SHA256SUMS"), format!("{hash}  recall\n")).unwrap();

    let bin_dir = dir.path().join("bin");
    let script = repo_root().join("scripts/install.sh");
    let out = std::process::Command::new("sh")
        .arg(script.to_str().unwrap())
        .arg(release.to_str().unwrap())
        .env("BINDIR", &bin_dir)
        .output()
        .expect("sh must run");
    assert!(
        out.status.success(),
        "install.sh failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Checksum verified"), "{stdout}");
    assert!(bin_dir.join("recall").exists(), "binary must be placed");
}
