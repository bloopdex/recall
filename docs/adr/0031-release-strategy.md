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
- **Installer that modifies PATH/shell config** — rejected for the
  original release: violates the explicit-integrations principle
  (ADR-0017/0020). Partially superseded by the amendment below for the
  Windows user PATH only.
- **Man pages** — rejected (see above).
- **A `recall uninstall` command** — rejected: deleting the binary from
  inside the running binary is platform-fragile; the four documented
  actions are simpler, safer, and honest about what each removes. The
  amendment below adds an `uninstall.ps1` SCRIPT instead (runs from the
  bundle/scripts, never from the installed binary), which removes the
  binary and the PATH entry — the manual actions remain documented.

## Amendment (post-release dogfooding, 2026-08-15) — Windows user PATH

Real dogfooding showed the PATH guidance in the installer was friction:
a first-time Windows user had to paste a
`[Environment]::SetEnvironmentVariable(...)` line before `recall` worked
globally. That is exactly the kind of step that should be automatic —
it is not an integration (it does not observe, capture, or hook
anything); it only makes the installed program findable.

Decisions:

- **The Windows installer now appends the bin directory to the USER
  PATH by default**, via the standard per-user environment-variable
  API. No administrator rights are required; the SYSTEM PATH is never
  read for modification and never written; shell profiles and startup
  files are never touched.
- **`-SkipPath` opts out** — scripted/CI installation stays
  deterministic and safe; the opt-out keeps the original
  explicit-anything-extra principle available.
- **Strict no-surprise rules** (implemented in `scripts/path.ps1`,
  test-pinned): the entry is only ever APPENDED; existing entries are
  preserved byte-for-byte (including empty entries); duplicates are
  detected case-insensitively, trailing-slash-insensitively, and after
  environment-variable expansion, and never added twice; idempotency
  is pinned by test.
- **The installer reports exactly what changed** (binary copy, PATH
  addition or "already present"), what did NOT change (SYSTEM PATH,
  profiles, hooks, database, model), and when a new terminal is needed
  for the change to take effect.
- **Linux/macOS installers deliberately do NOT modify PATH or shell
  profiles**: POSIX has no per-user environment-variable API, so the
  only equivalent would edit `~/.bashrc`/`~/.zshrc` — which the
  explicit-integrations principle forbids. The installer prints the
  one-line guidance and explains the asymmetry.
- **`scripts/uninstall.ps1`** completes the Windows story: removes the
  Recall bin directory from the USER PATH (preserving every other
  entry byte-for-byte), deletes `recall.exe`, and removes the bin
  directory only when empty. It never touches the database, the model,
  or the integrations — those explicit steps stay exactly as the
  original decision documents. Idempotent, test-pinned.

## Amendment 2 (2026-08-16) — tag-driven GitHub Releases

The publish step is now a workflow, not a manual action. Decisions:

- **Publishing is triggered by pushing a `vX.Y.Z` tag** (`.github/workflows/release.yml`, `on: push: tags: ['v*.*.*']` only). Ordinary branch pushes and pull requests never run the release workflow, and the workflow itself re-verifies `ref_type == 'tag'` plus the strict `^v\d+\.\d+\.\d+$` format before doing anything.
- **The tag is the request; Cargo.toml is the gate.** The version is extracted from the tag and must equal the `version` in Cargo.toml (strict literal match — anything else fails loudly before a release is created). The workflow never creates or moves tags; a release can only exist for a tag the maintainer pushed.
- **The workflow reuses the existing tooling**: the same validation suite as the release checklist (fmt, clippy with warnings denied, full tests, zero-network guard + gated download build), then `scripts/release.ps1 -Version <tag>` for the bundle, then an independent SHA256SUMS re-verification, then `gh release create` with the eight bundle files attached (binary, SHA256SUMS, install.ps1/sh, uninstall.ps1, path.ps1, CHANGELOG, LICENSE). Release notes are the existing CHANGELOG file — no generated or invented content.
- **Minimal permissions and no repository writes**: the workflow has `contents: write` (release creation only) and asserts `git status` stays clean — it never commits, pushes, or uploads `dist/` into the repository.
- **`v1.0.0` is untouched** and remains the historical release tag; this amendment changes only how future tags get published.
- Publication still requires a configured git remote; until the repository has one, the local checklist steps 1–8 remain the complete pre-release gate and the generated `dist/` bundle remains the local artifact.

## Consequences

The release is reproducible from a clean checkout by following the
checklist; artifacts are verifiable (checksums, smoke test); installs
are explicit and reversible; upgrades and old exports are pinned by
tests. The remaining gap — actual publication — is one documented step
blocked on the SOT hosting decision, not on Recall engineering.

After the amendment: on Windows, download → install → new terminal →
`recall` works end to end with no manual steps; the user PATH change
is minimal, reversible (`uninstall.ps1`), and fully transparent in the
installer output. The explicit-integrations principle is unchanged for
everything that is actually an integration (shell, git, embeddings).
