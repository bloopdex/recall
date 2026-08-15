# Dogfooding Recall

Recall v1.0.0 is feature-complete and release-ready. Before anything
new is built, the product needs weeks of real, daily use by its author
on real engineering problems. This document is the guide for that
period: what to do, what to record, and how to decide what belongs in
v1.1.

## Purpose

- Find the friction that only real use exposes: the moment a prompt
  annoys, an error confuses, or a search disappoints.
- Verify the core promise: *solve it once, find it again in seconds,
  months later.*
- Build confidence in the release: upgrades, backups, and the
  integrations must survive the author's actual workflow.
- Do NOT use this period to add features. Observations are collected;
  decisions come later, on evidence.

## Recommended daily workflow

1. **Capture on every fix.** Any problem that took more than five
   minutes to diagnose gets a capture — via the shell hook after a
   failed command, via the git hook after the fix commit, or plain
   `recall capture`.
2. **Search before you re-derive.** Whenever a problem looks familiar,
   search first. The whole point is that the search succeeds.
3. **Use the integrations every day.** Keep the shell hook installed in
   PowerShell and the git hook in at least one actively-developed
   repository.
4. **Check once a week.** `recall check` + a `recall export --path`
   backup. A dogfooding store should never be un-backup-able.
5. **Edit when a memory ages badly.** A wrong or unclear solution is a
   product problem too: `recall edit <id>`.

## Installation workflow (test this end to end)

1. Build the bundle: `powershell -File scripts\release.ps1 -Version 1.0.0`.
2. Install from it: `powershell -File install.ps1 -From
   dist\recall-1.0.0-windows-x86_64`.
3. Read the installer output in full — the "what changed / what was NOT
   touched / verify / next" section must be self-explanatory to a
   first-time user, including the USER-path addition (or
   "already present") and the new-terminal note.
4. Open a new terminal and run `recall version` — the USER path change
   made by the installer must make `recall` work globally with zero
   manual steps. (`-SkipPath` reproduces the old manual-PATH behavior
   for scripted installs.)
5. Uninstall: `powershell -File uninstall.ps1` — the binary and the
   Recall path entry are removed, everything else in PATH survives,
   memories and integrations are untouched (reported as such).
6. Note anything that required reading the README. If a step needed
   documentation, the output itself may need fixing.

## First-run workflow (test this on a fresh machine/account)

1. `recall capture` with no existing database — the welcome banner
   prints once, answers "what is this / where is my data / is anything
   sent over the network", and lists the three starter commands.
2. Complete the capture interactively: Problem, then Solution, then
   ✓ Saved.
3. `recall search "<words>"` immediately after.
4. `recall --help` — can the command groups replace the README for a
   new user?
5. Confirm nothing in the flow blocks automation: the same commands
   piped (`echo p | recall capture --solution s`) print no banner, no
   prompts, no unicode.

## Commands to use

| Command | Use it when |
|---|---|
| `recall capture` | any solved problem |
| `recall capture --from-shell` | a command just failed |
| `recall capture --from-ci` (via the GitHub Actions step) | a CI job fails |
| `recall search "<words>"` / `--project` / `--explain` | before re-deriving anything |
| `recall list` / `recall edit <id>` | store review and cleanup |
| `recall archive <id>` / `unarchive` / `delete <id> --yes` | lifecycle hygiene |
| `recall projects` | per-project overview |
| `recall export --path <file>` / `import` | weekly backup; machine moves |
| `recall embeddings status` / `build` / `download` | semantic layer care |
| `recall check` | weekly integrity pass |
| `recall shell install/uninstall/status` | shell hook upkeep |
| `recall git install/uninstall/status` | git hook upkeep |
| `recall version` | any support-style question |

## Recording friction — evidence rules

When something feels wrong, record it immediately (a Logseq capture is
the point). Each note must contain:

1. **The exact command** (with any flags) and the exact output —
   redacted if it contains secrets, but otherwise verbatim.
2. **The expectation:** what you thought would happen.
3. **What happened** and why it surprised you.
4. **Environment:** Windows/PowerShell or Bash, terminal emulator,
   TTY vs piped, database size at the time.
5. **The cost:** seconds lost? a wrong decision? data concern?
6. **Frequency:** first time or recurring.

A note without the exact command and expected-vs-actual is an anecdote,
not a finding. Re-record the same friction every time it repeats — the
count is the severity signal.

## Product bug vs expected behavior

Classify every note before acting on it:

- **Confirmed problem** — violates a documented guarantee or a pinned
  test; e.g. silent data loss, a captured memory that cannot be found
  by its own words, an integration that breaks the shell or git. Fix
  immediately.
- **Minor friction** — behavior is correct but unpleasant: wording,
  spacing, an extra prompt, a confusing hint. Collect, do not fix on
  the spot (it may resolve once habit forms).
- **Observation** — interesting, no judgment yet: e.g. most searches
  are single-word, most captures happen from the shell hook. Feeds
  v1.1 prioritization.
- **Future idea** — deliberately out of scope for v1.0 (fuzzy
  matching, pre-emptive surfacing, DeployScore feed). Record and
  move on.

Expected behavior that is NOT a bug: dedup skips within 30 days;
archived memories do not appear in default search; `--from-ci` requires
`--solution`; deletions need `--yes` when piped; semantic search needs
the model downloaded; busy-file errors under heavy contention (retry
succeeds — that is the guarantee).

## What NOT to change immediately

During dogfooding, resist changing:

- the database schema and the export format (compatibility contracts);
- search ranking and dedup rules (measured, ADR-pinned — changes need
  evidence of regression, not preference);
- the privacy model: whitelists, sanitizer patterns, fail-closed
  gates, zero-network enforcement;
- shell/git/CI integration architecture and the install/release flow;
- the exit-code contract (0/1/2) and piped output formats — scripts
  depend on them.

One exception: a **confirmed problem** that threatens data or breaks
the developer workflow is fixed immediately, with a regression test,
outside any version-planning discussion.

## v1.1 candidate criteria

A dogfooding finding becomes a v1.1 candidate when it meets all of:

1. it was recorded with evidence at least twice, in real use;
2. it addresses a *problem* (time, confusion, risk), not a
   preference;
3. it does not violate the v1.0 contract (schema, export format,
   privacy model, zero-network) — or it explicitly proposes a
   versioned, ADR-documented change;
4. it is small enough to ship with a test, or it is sliced into such
   pieces.

Known candidates already parked from v1.0 development (do not
re-litigate without new evidence): fuzzy/typo-tolerant matching,
pre-emptive surfacing in the shell prompt, the DeployScore incident
feed (ADR-0029 revisit conditions), encryption at rest (ADR-0026
revisit conditions), a hosting decision for release publication.

## Pre-dogfooding UX audit (2026-08-15)

Friction identified by walking every workflow before this period
started, classified per the rules above:

- **Confirmed problem (fixed before dogfooding):** search results
  printed internal ranking scores (`fused`) by default — noise for
  every real user. Ranking data is now `--explain`-only.
- **Confirmed problem (fixed):** no first-run orientation — a first
  command on a fresh machine dropped the user into a bare prompt with
  no answer to "what is this, where is my data, is anything sent
  anywhere". Added the one-shot, TTY-only welcome banner.
- **Minor friction (fixed):** installer output mixed "what happened"
  with "what to do next" and never stated what it did NOT change;
  success lines across commands were inconsistent in shape. Installers
  now summarize changed/unchanged + verification, and the CLI uses one
  consistent icon vocabulary (TTY-only, `RECALL_PLAIN=1` fallback).
- **Minor friction (fixed):** `--help` listed commands in
  implementation order. Grouped by concept via the command-map section
  and display ordering.
- **Observation:** capture prompts (`Problem: `) are functional but
  flat; made the interactive form two-line ("Problem" / `→`), keeping
  the piped form byte-identical for scripts.
- **Observation:** errors were one flat line. Added `✗`/hint
  presentation with a busy-lock retry hint; the recovery model was
  already embedded in corruption errors.
- **Future idea (unchanged):** anything from the v1.1 candidates list
  above.
