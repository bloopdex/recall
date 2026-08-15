# GitHub Actions integration — `recall capture --from-ci`

The CI integration is **opt-in and workflow-author-controlled**: a
dedicated step, gated by `if: failure()`, pipes a bounded log tail into
Recall and names the remediation the author knows. Recall stores fixes,
not raw CI events (ADR-0030).

## The pattern

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Build
        # Write the log to a file so the failure step can read its tail.
        run: cargo build 2>&1 | tee build.log

      - name: Capture failure in Recall
        if: failure()
        env:
          RECALL_DB_PATH: ${{ vars.RECALL_DB_PATH }}   # optional, see below
        run: |
          tail -n 100 build.log | recall capture --from-ci \
            --step build \
            --solution "re-run: this job flakes on cold caches"
```

What happens:

- The step runs only when a previous step in the same job failed.
- The problem is built deterministically from the whitelisted GitHub
  environment: `CI failure in <workflow> / <job> / step <step>
  (<event>)` — run ids are deliberately excluded, so repeated failures
  of the same job deduplicate (ADR-0011).
- The piped log tail becomes the `error` field; run metadata (run id,
  attempt, ref, sha) lands in `context`.
- The project label is the repository name (`GITHUB_REPOSITORY`).

## Requirements

- `recall` must be on PATH in the runner (install it in an earlier step
  from the release bundle, or `cargo install`).
- `--solution` is REQUIRED. If you don't yet know the fix, don't add
  the step — capture it later with the post-commit hook
  (`recall git install`) after you commit the fix.
- The database is local to the runner: without `RECALL_DB_PATH`, each
  run uses a fresh database (captures are lost with the runner). To
  keep captures, point `RECALL_DB_PATH` at a persisted location
  (e.g. a repository variable pointing to a mounted volume — your
  choice; Recall never transmits anything).

## Privacy rules (enforced, not advisory)

- Only the 10 whitelisted `GITHUB_*`/`RUNNER_OS` variables are read
  (pinned by test). CI secrets are never collected.
- The log tail passes the secret sanitizer (ADR-0018). In
  non-interactive CI, any detected secret fails closed: nothing is
  stored and the decline is printed.
- Keep the tail small (`tail -n 100`): Recall truncates captured text
  to 10,000 characters anyway — it stores incident context, not logs.
- `--include-secrets` is not available here: the CI path has no export
  step. Exports redact by default (ADR-0024).

## Failure behavior

The capture step itself cannot break the workflow result: it runs after
the failure, and its own exit code does not change the job's verdict.
If Recall is unavailable, the step fails with a clear error — add
`|| true` to the command if you want it fully silent.

## Deduplication

Repeated failures of the same job within 30 days produce ONE memory
(the second capture reports "Skipped: near-identical memory"). After a
fix and a later regression beyond the window, a new memory is created —
that is the model working as designed (ADR-0011). Use `--force` if you
deliberately want a new memory for the same failure.

## Which CI systems are supported

GitHub Actions only (the whitelist is GitHub-specific and was not
tested elsewhere — Recall does not claim unverified support). The
pattern (a failure-gated step piping a bounded tail into
`recall capture --from-ci`) transfers to any system that provides the
equivalent hook, but the environment whitelist would need to be
extended deliberately, with tests.
