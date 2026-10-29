# Partial pasted shortcut import and honest feedback (2026-10-29)

## Starting evidence
Released baseline v0.2.54, main `3c193abea1e574226e43dda6a9fa9874c143296e`, successful Windows CI 40884556428, no open PR. Earlier research already covers predictable user control and safe import in `docs/research/2026-10-28-import-feedback.md`, `2026-10-28-bulk-import-scaling.md` and `2026-10-28-last-import-filter-state.md`; avoid duplicating the broad search.

Source-confirmed failure: the pasted-list parser returned early on one malformed or relative file path before submitting any of the correctly selected paths to the existing per-file worker. The worker already retained individual errors, so the UI's fail-all prevalidation defeated its more resilient behavior. Independently, the mixed-result notice replaced the successful import count with an error. This is a reproducible source defect, not an observed customer complaint.

Comparable product context: PowerToys Run issue [#4212](https://github.com/microsoft/PowerToys/issues/4212) describes missing shortcuts despite others being present, but not this exact error. Findlater & McGrenere, CHI 2004 (https://doi.org/10.1145/985692.985704) motivates predictable explicit user choices; it is not a measurement of Desktop Pilot user time.

## Test and change
`src/ui/import_ownership_tests.rs` uses the actual SQLite-backed Settings, Windows URL parser, manual protected placement, history and Undo. One pasted batch contains a valid URL shortcut, an unsupported .txt file, a relative path and a duplicate. The new regression requires one saved new shortcut, two independently reported failures, no invalid file ever reaching the Windows shortcut parser, no unexpected placement, unchanged original bytes and protected preferences, and an operational Undo. A second test drives asynchronous Settings/poll_jobs and requires the added count and error to remain visible together.

The parser now only splits, strips Windows quoting and deduplicates. The existing asynchronous worker checks each path is absolute and has an allowed extension before any native decoding, then collects per-file failures without discarding valid choices. All accepted files still use the same SQLite transaction, identity index, ownership guard and per-item error path. There are no new settings, dependencies, telemetry, profile migrations or additional workers.

## Reproduction and boundaries
Run the new test against the unchanged 0.2.54 parser first: it must fail on the mixed pasted selection. Formatting or compilation failure is not behavioral evidence. After correction, require both unchanged Windows CI runners to pass Rust tests, README, formatting, Clippy strict, native integration, locked build and nine publication guards; then PR/main/release/tag/asset verification. These are synthetic selections on real program paths, not a measured human task-time saving or a physical Windows 11 200% DPI test. Retain issue #22 for cold startup and real network outages.
