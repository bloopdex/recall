# Release checklist

Reproducible, in order. Every box is checked by a concrete command; the
commands are the checklist. Run from the repository root
(`A:\BloopLab\Recall`).

## 1. Version consistency

- [ ] `codebase\recall\Cargo.toml` `version` matches the release version
      (the release script enforces this too).
- [ ] `CHANGELOG.md` has an entry for the release version with the date.
- [ ] `recall version` output makes sense: application version = release
      version; database schema v3; export format v1; model
      all-MiniLM-L6-v2 (model version 1).

```powershell
cd codebase\recall
cargo run --quiet -- version
```

## 2. Validation suite (all must pass)

- [ ] Tests: `cargo test --all-targets` — every suite green.
- [ ] Formatting: `cargo fmt --all -- --check`.
- [ ] Clippy: `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Zero-network: `powershell -File ..\scripts\check_no_network.ps1`
      (the in-test tree scan runs automatically with the tests).
- [ ] Download feature still compiles: `cargo build --features download`.
- [ ] Clean working tree: `git status` empty before tagging.

## 3. Release build + bundle

- [ ] `powershell -File ..\scripts\release.ps1 -Version <version>`
      (builds release, assembles `dist\recall-<version>-windows-x86_64\`
      with binary, SHA256SUMS, install scripts, CHANGELOG, LICENSE, and
      smoke-tests the binary).
- [ ] Bundle smoke: `dist\recall-<version>-windows-x86_64\recall.exe
      --version` prints the release version.

## 4. Install / uninstall

- [ ] Install: `powershell -File ..\scripts\install.ps1 -From
      dist\recall-<version>-windows-x86_64 -BinDir <temp>` — checksum
      verifies, binary lands in the bin dir, guidance printed.
- [ ] Idempotent: run it twice — second run succeeds.
- [ ] Tamper check: flip a byte in the bundle's recall.exe, install with
      checksums on → refuses.
- [ ] Uninstall: delete the installed binary, `recall shell uninstall`,
      `recall git uninstall` in each repo — the database and the model
      are never touched by uninstall (documented in README).
- [ ] (Linux/macOS, on those hosts) `sh scripts/install.sh <release-dir>`
      with the per-OS build.

## 5. Upgrade paths

- [ ] Upgrade suite green (in step 2's tests): v1→v3, v2→v3, migration
      failure atomicity, pre-migration backup restore.
- [ ] Old-export compatibility: `fixtures/upgrade/phase5-export.json`
      imports cleanly into a fresh database
      (`cargo test --test upgrade_paths`).

## 6. CLI smoke (release binary)

- [ ] `capture`, `search`, `list`, `check`, `version`, `projects`,
      `export`/`import` roundtrip on a throwaway `--db`.
- [ ] Exit codes: 0 success, 1 runtime error, 2 usage error (see
      `tests/cli_hardening.rs`).
- [ ] `--help` and every subcommand help render (the help text IS the
      reference manual — man pages deliberately rejected, ADR-0031).

## 7. Integrations

- [ ] Shell: `recall shell install` / `status` / `uninstall` roundtrip
      (PowerShell and Bash — covered by the suite).
- [ ] Git: `recall git install` / `status` / `uninstall` roundtrip in a
      scratch repo (covered by the suite).
- [ ] CI: the documented GitHub Actions fragment in
      `docs/ci/github-actions.md` matches the implemented `--from-ci`
      behavior (covered by `tests/ci_capture.rs`).

## 8. Documentation

- [ ] README quick start, security model, roadmap match actual behavior
      (no documented-but-unimplemented features).
- [ ] `docs/release/` (this checklist), `docs/ci/github-actions.md`,
      `docs/development/README.md`, `docs/database/README.md`,
      `docs/development/benchmarks.md` current.

## 9. Tag + publish

- [ ] `git tag v<version>` on the verified commit; `git status` clean.
- [ ] Publish: **pending the hosting decision** (SOT open decision #3) —
      upload the bundle directory contents, attach SHA256SUMS, point the
      install docs at the download URL. Until then the bundle directory
      is the release artifact.

## 10. Post-release

- [ ] Update the Logseq Recall page + Source of Truth with the release
      state (done as part of the phase documentation).
- [ ] `recall check` on the author's real database after upgrading the
      daily driver.
