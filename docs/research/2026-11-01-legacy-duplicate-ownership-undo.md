# Legacy duplicate quick shortcuts: reversible exclusion — 2026-11-01

## Source observation and product decision

The earlier offline-stability investigation ([duplicate ownership](2026-11-01-duplicate-ownership-offline-stability.md)) established that old profiles can contain repeated ownership rows for one application. Repeated records have another deterministic consequence in the existing transaction path: Apply backed up each removed row as before-<app-id>.<extension>. Two rows for the same application therefore targeted the same create-new backup filename. The second backup failed before placement, so an explicit Never choice could not be applied. The failure was independent of the previously corrected capacity count.

A further ambiguity is two entries for the exact same desktop path with conflicting saved manual/ownership metadata. Selecting one automatically could override a deliberate placement. The implementation now coalesces only entries that become identical *after the production file-identity check*; conflicting records for one physical path produce an actionable error before file mutation. Two **distinct** desktop shortcut copies belonging to the same application keep separate backup files, allowing one explicit Never decision to remove them both, then Undo to restore both.

This is a source-level defect and a targeted synthetic scenario, **not** a measured production incident or evidence of end-user time savings. The existing [adaptive shortcut research](2026-10-26-adaptive-shortcuts.md) includes original HCI research about control, predictability and layout stability; the rationale is that manually requested removal and reversibility must not depend on legacy bookkeeping quirks.

## Actual implementation tests and invariants

The integration regressions in src/engine/legacy_owned_duplicate_tests.rs use actual .url source files, duplicate State.owned records in the SQLite Store, production native shortcut parsing, preview, apply, reconciliation, durable journaling and rollback:

1. Two *identical* records for one physical copy, explicit Never, Apply, Undo. Expect one managed file to be removed and restored once, with the original intact.
2. Two *different* managed copies for the same application, explicit Never, Apply, Undo. Expect separate backup files and both copies restored, with the source intact.
3. One physical copy with conflicting manual metadata. Expect Apply to fail safely without changing ownership rows or either file.

Before correction the first two cases reached the same backup filename; the third case had no preflight metadata conflict check. The patch changes only these paths, not scoring, permissions, monitoring, history retention, network activity or the UI layout.

## Validation and remaining work

Run the normal Windows CI on the exact change commit: formatting, locked Rust suite, strict Clippy, locked build, native Windows integration and release-workflow guards, plus a merged-main check before distribution. No runner result should be inferred from this document. Visual UI testing on Windows 11 (including high DPI), a genuine disconnection/reconnection, and rare divergent-profile conflict recovery remain open.

For conflicting legacy records the conservative immediate behavior is to preserve the files and ask for manual review rather than guess which metadata is authoritative. A dedicated guided reconciliation UI is not added without evidence that this is frequent enough to justify additional complexity.
