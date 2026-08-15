# ADR-0024 — Export/import: portable JSON with no lock-in

- **Date:** 2026-08-15
- **Status:** Accepted

## Context

The original Phase 5 page scopes export/import explicitly: "an
engineering memory must never be trapped in its tool", with a portable
JSON format, a schema version field, no internal IDs in the format, and
secrets redacted by default. Phase 0's privacy design already promised
"exportable: portable JSON (opt-in)".

## Decision

- **Format:** a JSON envelope — `format_version` (currently 1),
  `exported_at`, `recall_schema_version`, and `memories[]` with the
  named domain fields plus lifecycle `status`. **No internal database
  ids** — an export is inspectable data, not a dump, and can be edited
  or diffed by hand.
- **Embeddings are not exported.** They are derived data tied to a
  model/version (ADR-0015); importing them would import vectors Recall
  cannot trust. After import, `recall embeddings build` rebuilds them
  locally. The export carries `recall_schema_version` so future format
  evolution can migrate old files explicitly.
- **Secrets redacted by default:** every text field passes through the
  Phase 4 sanitizer before serialization; `--include-secrets` exports
  raw text as an explicit opt-in (Phase 0: export is opt-in; raw
  secrets never leave by accident).
- **CLI:** `recall export [--path FILE] [--include-secrets]` (stdout by
  default — logs moved to stderr in this phase so stdout stays clean
  JSON), `recall import FILE [--force]`.
- **Import semantics:** strict `format_version` check; all-or-nothing
  entry validation (a single invalid entry aborts with its index);
  duplicate detection by (project, normalized problem) — duplicates are
  skipped by default and reported, `--force` imports them anyway;
  `captured_at` and `status` are preserved (an archived memory imports
  archived).
- Multi-machine sync stays out of scope (local-first): export/import is
  backup and migration, not synchronization.

## Alternatives considered

- **JSONL stream** — rejected: the envelope carries format/version
  metadata; an array of objects at personal scale is fine.
- **Raw SQLite file copy as the official format** — rejected: version-/
  OS-coupled, not inspectable, and it's already trivially available to
  the user as a manual backup; the JSON format is the portable contract.
- **Exporting embeddings** — rejected (see above).

## Consequences

Backups and machine moves are a documented, tested workflow
(round-trip test preserves fields, timestamps, and archived status).
The format is stable at version 1; any breaking change bumps
`format_version` and import rejects versions it cannot read.

## Amendment (Phase 6, 2026-08-15) — import hardening

Three hardening fixes, all test-pinned:

1. **Future-schema exports are refused.** Import previously checked only
   `format_version`; an export from a NEWER Recall would have had its
   unknown fields silently dropped by serde — a lossy import. Now
   `recall_schema_version > current` fails with an upgrade message.
2. **All-or-nothing now covers every field.** Timestamp and lifecycle
   status errors were discovered mid-insert-loop, meaning a bad entry
   late in the file left earlier entries inserted. All entries are now
   fully validated (required fields, `captured_at` parse, strict
   status) before the first insert; the property suite
   (`tests/properties.rs`) proves random malformed files never panic
   and never partially write.
3. **Strict status parsing on import.** Unknown lifecycle statuses were
   silently defaulted to `active`; they are now rejected with the entry
   index. Non-UTF-8 files get a "not a valid Recall export" error
   instead of an obscure read error.
