# ADR-0026 — Encryption at rest: rejected for Phase 6

- **Date:** 2026-08-15
- **Status:** Accepted (rejection, with revisit conditions)

## Context

The original Phase 6 page and the BloopLab security standard list
"encryption at rest option" as Recall hardening work. The phase contract
also says: "If the project cannot provide a safe key-management model,
explicitly reject encryption rather than implementing a weak design."
Phase 6 researched the option against Recall's actual threat model
before deciding.

## Research

- **Threat model.** Recall is a single-user local CLI. The database
  contains what the user captured — including any secrets they chose to
  store. Secrets are NOT the primary storage concern: redaction
  (ADR-0018) is the primary defense and applies at capture/export. What
  encryption at rest would add: protection against offline attackers
  (stolen disk, physical access) and forensic inspection of the file.
  It cannot protect against malware running as the user — the key is
  reachable in the same session.
- **SQLCipher** (the standard open-source option): a SQLite fork with
  page-level AES-256, ~5–15% official overhead. Costs for Recall:
  (1) a fork dependency — rusqlite must be swapped for the `sqlcipher`
  crate lineage; (2) the database file becomes unreadable by plain
  `sqlite3` tooling (breaks the "boring technology" inspectability and
  portability of manual backups); (3) the ecosystem shows maintenance
  churn (e.g. the .NET sqlcipher bundle was dropped upstream for lack of
  maintenance); (4) every existing Recall database would need a
  migration story ("never make existing users lose access").
- **SQLite SEE** — commercial license (~$2000 class), rejected on
  licensing grounds for a personal tool.
- **Key management (the hard part):**
  - *OS keychain / DPAPI + libsecret:* needs per-OS native dependencies
    (new `windows` crate etc.), and on Windows a DPAPI-encrypted key
    file next to the database protects only against attacks where the
    OS account is locked — the offline-theft threat is already mostly
    covered by Windows account security in that scenario.
  - *User passphrase:* breaks Recall's core UX. The post-commit hook and
    the prompt hook run `recall capture` non-interactively; a passphrase
    prompt makes every hook invocation fail or hang. Caching the key in
    a file next to the DB = the key travels with the ciphertext =
    no protection.
  - *Application-level field encryption:* encrypting columns breaks
    FTS5/vec0 indexing — the entire search layer would need a redesign.
    Not viable.
- **Performance:** the 5–15% overhead would be acceptable; performance
  was never the blocker. The blocker is key management without breaking
  the zero-friction capture contract.

## Decision

**Reject encryption at rest for Phase 6.** Recall keeps its plaintext
SQLite database (protected by OS account permissions as today), and
relies on its documented, tested defenses:

1. **Redaction-first privacy** (ADR-0018) — the secret-detection layer
   at capture, export, and shell/git context.
2. **Confirmed capture gates** — anything redacted requires explicit
   confirmation before persistence.
3. **Portable redacted exports** (ADR-0024) — the escape hatch for
   users who want sensitive data OUT of the database entirely.

The decision and its reasoning are documented in the README security
model, and the SOT is updated to record the rejection.

**Revisit conditions:** (1) multi-user or networked deployments appear
(in Phase 7 or beyond); (2) a key-management story arrives that keeps
the hook flows working — e.g. an explicit opt-in per-database mode with
a documented keyfile discipline that the user chooses knowing the
trade-off; (3) SQLCipher-aligned builds become first-class in the
rusqlite ecosystem with a plain migration path.

## Alternatives considered

All researched and rejected above: SQLCipher (fork + tooling + migration
costs), SEE (license), OS-keychain key files (protection level mismatch
with the dependency cost), passphrase (breaks non-interactive hooks),
field-level encryption (breaks search indexing).

## Consequences

No encryption surface to get wrong, no false sense of security, no risk
of locking existing users out. The honest limitation — a stolen disk can
be read offline — is stated plainly in the docs, with the mitigation
(export redacted or delete what must not sit on disk). The security
standard's "encryption-at-rest option" line is amended to reference this
ADR's rejection and revisit conditions.
