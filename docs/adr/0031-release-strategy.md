# ADR-0031 — Release & distribution strategy: local-first artifacts, versioned surfaces

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

Phase 7 turns Recall into a distributable release: versioning,
artifacts, installation, upgrade/uninstall behavior, release
documentation. Hard facts from research: the repository has **no git
remote and no hosting decision** (SOT open decision #3), so automated
publishing (GitHub Releases, winget, cargo-dist) has no destination
today. The release engineering must therefore be complete and
reproducible locally, with publication a documented final step for
whenever hosting is chosen.

## Decision

- **Versioning — four independent surfaces (pinned by `recall
  version`):**
  - application: SemVer, currently **1.0.0** (Phases 0–7 complete;
    CLI/schema stable since Phase 5);
  - database schema: the migration max (3), bumping only with a new
    migration (append-only, ADR-0006);
  - export format: `format_version` (1), bumping only on breaking
    changes (ADR-0024);
  - embedding model: model id + model version (`all-MiniLM-L6-v2`, 1),
    stored per embedding row (ADR-0015).
  Changing one never implies another; `recall version` prints all four
  (informational commands never create a database).
- **Artifacts (Windows host):** a release bundle directory
  `dist/recall-<version>-windows-x86_64/` containing the release binary,
  `SHA256SUMS`, both install scripts, CHANGELOG, LICENSE — assembled by
  `scripts/release.ps1` (verifies the manifest version matches, builds
  release, checksums, smoke-tests the binary). Linux/macOS: the same
  build on the target OS (documented commands in the script header).
  Binary size / startup targets from the phase page: measured at release
  time and recorded, not gated.
- **Installation (explicit, never intrusive):** `scripts/install.ps1` /
  `install.sh` copy the binary into a user bin dir (default
  `~/.recall/bin`), verify `SHA256SUMS` when present (tampered binaries
  are refused — test-pinned), print PATH guidance, and NEVER touch
  PATH/shell profiles/integrations/database/model. Integrations stay
  explicit commands (`recall shell install`, `recall git install`).
  Scripts are ASCII-only: PowerShell 5.1 misparses UTF-8-without-BOM
  files containing non-ASCII (found and pinned during this phase).
- **Uninstall:** documented as four separate actions — delete the
  binary; `recall shell uninstall`; `recall git uninstall` (per repo);
  the database and the model are NEVER deleted by any script and
  require explicit manual action (an uninstaller that eats memories is
  unacceptable — the tool's own lifecycle rule, ADR-0023).
- **Upgrades:** existing migration guarantees are the upgrade story
  (v1→v3, v2→v3, failure atomicity, pre-migration backup restore); old
  exports stay importable (a committed Phase 5-era export fixture is
  pinned by tests/upgrade_paths.rs); no config files exist to migrate.
- **Man pages: rejected.** clap `--help` IS the reference manual — it
  is versioned with the binary, generated from the same source as the
  parser, and cannot drift from behavior the way static man pages do.
  Documented as the decision rather than silently skipping the TODO.
- **Release checklist:** `docs/release/RELEASE-CHECKLIST.md` — the
  reproducible, ordered gate (validation suite → release build →
  bundle → install/uninstall → upgrade → CLI smoke → integrations →
  docs → tag → publish).
- **OSS surface:** LICENSE (MIT), CHANGELOG, CONTRIBUTING, SECURITY.
  Issue templates deferred with the hosting decision (they configure a
  host).

## Alternatives considered

- **cargo-dist / automated GitHub Releases** — rejected for now: no
  remote exists; the local bundle + scripts are the complete mechanism
  and become the publish step's input unchanged.
- **winget / scoop / choco packages** — rejected for now: they require
  a hosted repo + manifests; revisit with the hosting decision.
- **Installer that modifies PATH/shell config** — rejected: violates
  the explicit-integrations principle (ADR-0017/0020).
- **Man pages** — rejected (see above).
- **A `recall uninstall` command** — rejected: deleting the binary from
  inside the running binary is platform-fragile; the four documented
  actions are simpler, safer, and honest about what each removes.

## Consequences

The release is reproducible from a clean checkout by following the
checklist; artifacts are verifiable (checksums, smoke test); installs
are explicit and reversible; upgrades and old exports are pinned by
tests. The remaining gap — actual publication — is one documented step
blocked on the SOT hosting decision, not on Recall engineering.
