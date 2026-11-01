# Recover sampling after a temporary SQLite writer lock — 2026-11-01

## Research and scope

Continuation of the [paused observation boundary](2026-11-01-paused-sample-evidence.md), [short-visit behavior](2026-10-26-short-foreground-visits.md) and [adaptive interface evidence](2026-10-26-adaptive-shortcuts.md), not a new compatibility matrix. The primary [SQLite WAL locking documentation](https://github.com/sqlite/sqlite/blob/master/doc/wal-lock.md) establishes that one writer can hold the lock and another writer may receive SQLITE_BUSY. [Findlater and McGrenere (CHI 2004)](https://doi.org/10.1145/985692.985704) provides a reason to avoid unexpected adaptive changes, not a performance measurement for this software. [A 2026 Fences user thread](https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop) reports unwanted icon rearrangement in another product; it is **not** evidence that Desktop Pilot users encounter this database lock.

## Source-confirmed problem and hypothesis

The monitor formerly propagated exhausted SQLite BUSY/LOCKED from `record_learning_if_permitted` and `persist_confirmed_usage` with `?`, ending the collection worker, even when a concurrent Settings write or backup would shortly release the writer. The foreground accumulator had already advanced its first-session marker before writing. A user who edits Settings while working in a second project could thus lose useful observation until worker restart. This is a source-confirmed control-flow issue; its real-world incidence is unknown.

The safer contract is to **discard an uncommitted sampling window** on persistent writer contention and require new genuine input, while keeping the monitor alive. Reuse existing bounded retries; do not retry corrupt settings, disk failures or other unexpected errors. Do not invent activity from lock-wait time.

## Production-path regression

The new tests in `src/engine/paused_usage_tests.rs` use the actual `Store`, WAL SQLite writer transaction, the production `UsageAccumulator`, `persist_confirmed_usage` and learning writer, plus a second real SQLite connection holding `BEGIN IMMEDIATE`. They test interrupted editor first sample, independent learning refusal, release of lock, fresh editor activity, project switching to browser, a quick return to editor, exactly one session per project, and no replay or phantom seconds. A separate corrupted-settings test verifies that non-lock database/configuration errors remain visible.

Expected synthetic before/after: before, temporary lock ends monitor via `?`, or causes the direct writer helper to error; after, the two sampling functions return a denied sample, no SQLite history is changed while blocked, and new confirmed input commits after unlock. The tests exercise real database code but synthetic time and identities. They do **not** measure user time savings or replace Windows 11 interaction, high-DPI GUI or real backup/load testing.

## Acceptance and follow-up

Require standard public Windows CI: format, locked Rust tests, strict Clippy, build, native integration, release guards on both runners; inspect actual run results before merging. The branch must not be released based solely on these unit-level outcomes. Remaining work: confirm multi-minute writer contention across heartbeat/maintenance writes, physical Windows 11 and screen scaling, suspend/resume, disconnected disks, visible status after a long outage and user-observed time-to-value.
