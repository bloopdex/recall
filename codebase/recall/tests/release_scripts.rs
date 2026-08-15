//! Install/uninstall-script behavior (ADR-0031 amendment).
//!
//! The install scripts are minimal by design: copy the binary into a
//! user bin directory, verify checksums when present, and — on Windows —
//! append the bin directory to the USER PATH (opt-out `-SkipPath`,
//! never the SYSTEM PATH, never shell profiles, never the database or
//! the integrations). The Windows tests run the real install.ps1 and
//! uninstall.ps1 against a fake release directory (the compiled test
//! binary plays the role of recall.exe) and pin: placement,
//! idempotency, checksum verification, tamper refusal, and the PATH
//! helpers (in isolation, with explicit values, so the real user PATH
//! is never written by a test). install.sh is exercised through `sh`
//! when available (gracefully skipped otherwise).

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
    // -SkipPath: the test suite must never modify the real user PATH.
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            script.to_str().unwrap(),
            "-SkipPath",
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
    assert!(stdout.contains("PATH: skipped"), "{stdout}");
    assert!(bin_dir.join("recall.exe").exists(), "binary must be placed");

    // Idempotent: a second install succeeds.
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            script.to_str().unwrap(),
            "-SkipPath",
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
            "-SkipPath",
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

/// Read the real USER PATH (read-only; tests never write it).
#[cfg(windows)]
fn user_path() -> String {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','User')",
        ])
        .output()
        .expect("powershell must run");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run one expression against scripts/path.ps1 with explicit -UserPath
/// values, so the real user PATH is never read for modification.
#[cfg(windows)]
fn path_helper(expr: &str) -> String {
    let script = repo_root().join("scripts/path.ps1");
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(". '{}'; {expr}", script.display()),
        ])
        .output()
        .expect("powershell must run");
    assert!(
        out.status.success(),
        "path helper failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(windows)]
#[test]
fn user_path_detection_is_case_insensitive_and_trims_slashes() {
    assert_eq!(
        path_helper(
            "Test-UserPathEntry -Entry 'C:\\recall\\bin' -UserPath 'A;C:\\RECALL\\BIN\\;B'"
        ),
        "True"
    );
    assert_eq!(
        path_helper("Test-UserPathEntry -Entry 'C:\\recall\\bin' -UserPath 'A;B'"),
        "False"
    );
    assert_eq!(
        path_helper("Test-UserPathEntry -Entry 'C:\\recall\\bin' -UserPath ''"),
        "False"
    );
}

#[cfg(windows)]
#[test]
fn user_path_detection_expands_environment_variable_tokens() {
    let expanded = format!("{}\\.recall\\bin", std::env::var("USERPROFILE").unwrap());
    let expr = format!(
        "Test-UserPathEntry -Entry '%USERPROFILE%\\.recall\\bin' -UserPath 'A;{expanded};B'"
    );
    assert_eq!(path_helper(&expr), "True");
}

#[cfg(windows)]
#[test]
fn adding_a_user_path_entry_appends_without_duplicates_and_preserves_the_rest() {
    // Pure append: the existing value is preserved byte-for-byte.
    assert_eq!(
        path_helper(
            "Add-UserPathEntry -Entry 'C:\\new' -UserPath 'C:\\Program Files\\x;%SystemRoot%'"
        ),
        "C:\\Program Files\\x;%SystemRoot%;C:\\new"
    );
    // Already present (any case, any trailing slash) -> unchanged.
    assert_eq!(
        path_helper("Add-UserPathEntry -Entry 'C:\\new' -UserPath 'A;C:\\NEW\\;B'"),
        "A;C:\\NEW\\;B"
    );
    // Empty user PATH -> just the entry.
    assert_eq!(
        path_helper("Add-UserPathEntry -Entry 'C:\\new' -UserPath ''"),
        "C:\\new"
    );
    // A trailing empty entry survives byte-for-byte.
    assert_eq!(
        path_helper("Add-UserPathEntry -Entry 'C:\\new' -UserPath 'A;B;'"),
        "A;B;;C:\\new"
    );
}

#[cfg(windows)]
#[test]
fn removing_a_user_path_entry_preserves_everything_else() {
    assert_eq!(
        path_helper("Remove-UserPathEntry -Entry 'C:\\old' -UserPath 'A;C:\\old;B;;C'"),
        "A;B;;C"
    );
    assert_eq!(
        path_helper("Remove-UserPathEntry -Entry 'C:\\missing' -UserPath 'A;B'"),
        "A;B"
    );
    assert_eq!(
        path_helper("Remove-UserPathEntry -Entry 'C:\\old' -UserPath 'C:\\old'"),
        ""
    );
}

#[cfg(windows)]
#[test]
fn install_ps1_skip_path_never_touches_the_user_path() {
    let dir = tempfile::tempdir().unwrap();
    let release = fake_release_dir(dir.path());
    let bin_dir = dir.path().join("bin");
    let before = user_path();
    let script = repo_root().join("scripts/install.ps1");
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            script.to_str().unwrap(),
            "-SkipPath",
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
    assert!(stdout.contains("PATH: skipped"), "{stdout}");
    assert_eq!(
        user_path(),
        before,
        "-SkipPath must leave the user PATH byte-for-byte unchanged"
    );
}

#[cfg(windows)]
#[test]
fn uninstall_ps1_removes_only_the_binary_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::copy(common::bin(), bin_dir.join("recall.exe")).unwrap();
    // A user file next to the binary must survive.
    std::fs::write(bin_dir.join("keep.txt"), "user data").unwrap();

    let before = user_path();
    let script = repo_root().join("scripts/uninstall.ps1");
    let run_uninstall = || {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-File",
                script.to_str().unwrap(),
                "-BinDir",
                bin_dir.to_str().unwrap(),
            ])
            .output()
            .expect("powershell must run");
        assert!(
            out.status.success(),
            "uninstall failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };

    let first = run_uninstall();
    assert!(first.contains("Removed:"), "{first}");
    assert!(!bin_dir.join("recall.exe").exists(), "binary must be gone");
    assert!(bin_dir.join("keep.txt").exists(), "user files must survive");
    assert!(bin_dir.exists(), "a non-empty bin dir is never removed");
    assert!(first.contains("Your memories were not deleted"), "{first}");

    // Idempotent: a second run succeeds and reports nothing to remove.
    let second = run_uninstall();
    assert!(second.contains("Binary: not present"), "{second}");
    assert_eq!(
        user_path(),
        before,
        "uninstall must never alter an unrelated user PATH"
    );

    // A bin directory holding ONLY the binary is removed with it.
    let lone_dir = dir.path().join("lone");
    std::fs::create_dir_all(&lone_dir).unwrap();
    std::fs::copy(common::bin(), lone_dir.join("recall.exe")).unwrap();
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-File",
            repo_root().join("scripts/uninstall.ps1").to_str().unwrap(),
            "-BinDir",
            lone_dir.to_str().unwrap(),
        ])
        .output()
        .expect("powershell must run");
    assert!(
        out.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !lone_dir.exists(),
        "an emptied bin directory must be removed"
    );
}
