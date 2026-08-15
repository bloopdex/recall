# ADR-0030 — CI failure capture: opt-in `recall capture --from-ci`

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

The Phase 7 roadmap calls for "automatic capture from CI failures".
The phase contract adds the constraints that matter: capture useful
incident context, not whole CI logs; preserve the core boundary ("How
did we solve this?" — not "store every CI event"); never persist
arbitrary CI environment; preserve the Phase 4 privacy model (redact →
show/confirm where interactive → fail closed where confirmation is
unavailable); keep integrations optional and workflow-safe.

## Research

- **Failure detection in GitHub Actions** has no job-status environment
  variable. The established pattern is a dedicated step gated by
  `if: failure()` (same-job step failures) — the step runs only when
  something before it failed. This is the natural, workflow-author-
  controlled opt-in point.
- **What GitHub provides to the failing step:** whitelisted `GITHUB_*`
  metadata (workflow, job, event, repository, sha, ref, run id/attempt,
  server URL) plus `RUNNER_OS`. Logs are NOT provided to the failure
  step automatically; the workflow author controls them (e.g. the build
  step writes a log file, the failure step pipes `tail -n 100` into
  recall).
- **The solution-time problem:** at failure time the fix does not exist
  yet. Capturing "unsolved" memories would violate the core boundary.
  The only honest shape: the failure step is written by someone who
  knows the failure class (flaky test → "re-run"; infra issue →
  "re-provision the runner") and provides the solution explicitly.
  Unknown fixes are captured later through the existing flows
  (`capture --from-git` via the post-commit hook).
- **Deduplication:** the existing model (normalized problem + project,
  30-day window — ADR-0011) is sufficient IF the auto-built problem is
  deterministic across runs. Therefore the problem text is built from
  workflow/job/event/step — deliberately NOT the run id/attempt, which
  go into the context field instead. No second deduplication system.
- **Privacy:** CI environments carry tokens and cloud credentials. The
  whitelist principle from Phase 4 extends: a new module reads exactly
  10 named variables (pinned by a dedicated security test); the piped
  log passes the existing sanitizer; in non-interactive CI, detected
  secrets fail closed (the confirmation gate declines — nothing is
  stored), exactly like `--from-shell`/`--from-git` without a TTY.

## Decision

- **New context mode:** `recall capture --from-ci [--step NAME]
  --solution "…"` inside a GitHub Actions `if: failure()` step; piped
  stdin carries the bounded log tail (the author truncates: `tail -n
  100`). `--solution` is REQUIRED (the core boundary). `--problem`
  overrides the auto-built text.
- **Auto-built problem:** `CI failure in {workflow} / {job} / step
  {step} ({event})` — deterministic, dedup-friendly.
- **Project:** `GITHUB_REPOSITORY`'s repo name (consistent with
  name-based identity, ADR-0021); falls back to local git detection.
- **Context:** run id, attempt, ref, sha — metadata only, never part of
  dedup.
- **No composite action** (`uses: …`): Recall has no hosted repo to
  publish one from; a documented workflow fragment in
  `docs/ci/github-actions.md` is the interface, and it degrades
  gracefully — the step is written so Recall's absence or failure
  cannot affect the workflow result (the step itself may fail harmlessly;
  users can append `|| true` as with the git hook pattern).
- **GitHub Actions is the only supported CI** until another system
  demonstrates the same pattern; the module name and whitelist make the
  scope explicit.

## Alternatives considered

- **Auto-capture every failed job with no step** — impossible without a
  GitHub App/webhook service (a network daemon — violates zero-network
  and local-first) and violates "store every CI event".
- **Capture with a placeholder solution ("fix pending")** — rejected:
  pollutes the store with unsolved rows and breaks the core boundary;
  `recall edit` exists for corrections to real captures.
- **Full-log capture with truncation only** — rejected: "useful
  incident context, not a log-storage system"; the author chooses the
  tail.
- **Support GitLab CI / other systems now** — rejected: unverified
  environments claim support Recall has not tested (Phase 4 principle);
  the whitelist is GitHub-specific by name.

## Consequences

CI failures become memories only when the workflow author opts in and
names the remediation; repeated failures deduplicate; secrets in logs
fail closed; CI runs can never be broken by Recall (the step is
author-controlled). Tests pin all of it (tests/ci_capture.rs, 6 tests,
plus the whitelist test in tests/security.rs).
