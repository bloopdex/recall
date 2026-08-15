# ADR-0018 — Shell output sanitization & privacy

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

Phase 4 introduces content Recall captures *without the user typing it
into a prompt*: a failed command line, piped error output, a commit
subject. Command lines in particular embed secrets routinely
(`--password=...`, tokens, connection strings). Phase 0's privacy
position — nothing sensitive stored, nothing leaves the machine — now
has to hold for automatically captured text.

## Decision

**Capture context → sanitize → show what will be saved → explicit
confirmation when anything changed → persist.** In that order.

- **Detection:** a fixed, conservative, explainable pattern set — no
  heuristic classifier:
  - `--secret-flag value` / `--secret-flag=value` for known secret flag
    names (password, token, secret, api-key, access-key, …)
  - `key=value` / `key: value` where the key equals or *ends with* a
    known secret key name (`DB_PASSWORD`, `AWS_ACCESS_KEY_ID` — suffix
    matching catches prefixed forms)
  - `Bearer <token>` and `Authorization: <value>` header forms
  - AWS access key ids (`AKIA` + 16 alphanumerics)
  - basic-auth URLs (`scheme://user:pass@host` — the password only)
  - PEM private-key blocks
- **Redaction:** matches become `<redacted>`; the sanitized text is what
  is shown and stored — never the original.
- **Confirmation:** when anything was redacted, the capture flow prints
  exactly what will be saved and requires an explicit `y` before
  persisting. Without a confirmation source (piped stdin in `--from-shell`
  mode — the pipe is already consumed as error text), capture **fails
  closed**: declined, nothing stored.
- **Limits:** command lines truncated at 1,000 chars, piped/auto-captured
  text at 10,000 chars, both with explicit markers.
- **Scope:** applies to auto-captured context (shell snapshot, piped
  error, commit subject). User-typed problem/solution are the user's
  explicit input and are not rewritten.

The guarantee Recall makes is narrow and stated honestly: *common secret
shapes never reach the database silently.* Arbitrary secret detection is
impossible, and the documentation says so.

## Consequences

- A command like `npm login --password hunter2` is stored as
  `npm login --password <redacted>` — searchable as *an npm login
  failure* without the secret.
- Scripted (piped) capture with secrets in the command line cannot
  complete without a terminal — the safe direction, at the cost of one
  manual re-run for that rare case.
- The pattern set is pure string logic in `domain/sanitize.rs`, unit
  tested per pattern, no new dependencies (the zero-network dependency
  guard stays green).

## Alternatives considered

- **ML/NER secret detection** — rejected: heavyweight, opaque,
  untestable guarantees for a personal CLI.
- **Silent redaction without confirmation** — rejected: destroys
  legitimate content without the user ever knowing.
- **Refuse any capture containing a secret pattern** — rejected:
  redaction + confirmation preserves the memory (the failure is real
  even if the secret is not) while keeping the secret out.
- **Sanitize only the command line, not piped output** — rejected:
  error output is the most likely place for tokens and stack dumps.

## Revisit conditions

Real-world false positives/negatives from actual shell usage. If a
pattern proves noisy, narrow it; if a common shape is missed, add it —
each change is a one-line pattern plus a unit test.

## Amendment (Phase 6, 2026-08-15) — pattern review

The Phase 6 review added high-signal, explainable patterns, each with
false-positive/limitation tests:

- **JWT tokens** (`eyJ…` + at least two dot-separated base64url
  segments; greedy scan so trailing sentence punctuation is not
  swallowed). Documented limitation: any `eyJ`-prefixed identifier with
  two dots is treated as a JWT (rare in log output, pinned by test).
- **Well-known hosted tokens:** GitHub (`ghp_`, `gho_`, `ghu_`, `ghs_`,
  `ghr_`, `github_pat_`), Slack (`xoxb-`, `xoxp-`, `xoxa-`, `xoxr-`,
  `xoxe-`, `xoxs-`), Stripe (`sk_live_`, `rk_live_`, `whsec_`).
  Case-sensitive prefixes (lowercase by convention), minimum 8-char
  tail, boundary-delimited. Documented limitations: tokens truncated to
  under the minimum are missed; `sk_test_` keys are deliberately NOT
  matched (test-mode keys are not production secrets).
- **Boundary set widened** with `)`, `]`, `}`, `{`, `:`, `.`, `!`, `?`
  so tokens at sentence ends, in parens, or after `label:` are found.
- **Basic-auth URL passwords containing `@`** are now redacted whole
  (userinfo ends at the LAST `@` before the first `/`, `?` or `#`).
  Documented limitation: a raw `/`, `?` or `#` inside a password
  (normally URL-encoded) leaks the tail.
- **Deterministic property tests** (`tests/properties.rs`, XorShift,
  no new deps): embedded secrets never survive sanitization across 200
  random contexts; clean text passes through byte-identical.

The guarantee is unchanged and stated again: common secret shapes never
reach the database silently — not that arbitrary secrets are detectable.
