# Security policy for Recall

## Reporting a vulnerability

Recall has no public distribution channel yet (repo hosting is an open
decision — see the Source of Truth). Until then, report suspected
vulnerabilities directly to the repository owner via the project page
links; do not open public issues for unpatched vulnerabilities.

## Supported versions

Only the latest release receives fixes. Upgrade paths from older
databases are covered by the migration suite (v1→v3, v2→v3, backup
restore); old export files remain importable (format versioning,
ADR-0024).

## Security model

The full model is documented in the README ("Security model" section)
and the ADRs. Summary:

- **Threat model:** single-user local CLI. Defenses target secret
  leakage (redaction, whitelists, confirmation gates), accidental
  network access (enforced in CI), and data loss (WAL + transactional
  writes, pre-migration backups, `recall check`, loud corruption
  failures with recovery guidance).
- **Encryption at rest is deliberately NOT implemented** — see ADR-0026
  for the research, the reasoning, and the explicit revisit conditions.
- **Secrets:** the sanitizer (ADR-0018) covers common shapes and is
  explicitly not a perfect detector. Auto-captured content is shown
  redacted and requires confirmation; non-interactive contexts fail
  closed. Exports redact by default.
- **CI integration:** `recall capture --from-ci` reads only the
  whitelisted `GITHUB_*` variables (test-enforced) and fails closed on
  detected secrets in logs.
- **Logs never carry memory content** (test-enforced).

## Disclosure expectations

Local-only bugs (UI quirks, performance) can be fixed without a
security advisory. Anything that leaks user data or secrets — in
exports, logs, shell/CI integration, or the database — is treated as a
security issue and fixed before release.
