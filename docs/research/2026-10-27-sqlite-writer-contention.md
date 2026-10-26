# A recoverable SQLite writer lock must not discard settings (2026-10-27)

## Starting state and evidence

Continuation of Desktop Pilot's existing local-only durability work. Branch based on PR #13 for 0.2.15 at commit `e97bc24a742ca5f845f3d886ddc3c20d2583a32b`; main was 0.2.14 (`b01c643a6f27656a8ae90b0bbe45e4143ef257ba`). The PR's Windows 2022 pull-request check encountered `database is locked` inside `store::tests::immediate_transactions_keep_concurrent_updates` after most tests passed, although a preceding push build had passed. This intermittent signal does not establish frequency for real users.

Primary references:
- SQLite explicitly documents that `BEGIN IMMEDIATE` may return `SQLITE_BUSY` when another writer is active: https://www.sqlite.org/lang_transaction.html
- SQLite's busy timeout has a finite bound and may return `SQLITE_BUSY` afterwards: https://www.sqlite.org/c3ref/busy_timeout.html
- SQLite serializes concurrent writers, including in WAL mode: https://www.sqlite.org/isolation.html

**Production finding:** `Store::transaction` correctly acquires an immediate write transaction before reading/modifying user settings, but abandoned the entire operation as soon as acquisition returned `SQLITE_BUSY` (including a brief lock that would clear moments later). Increasing or disabling the wait globally is undesirable for responsiveness; retrying arbitrary operations after partial effects is unsafe.

## Failure-first production-logic regressions

Two tests in `src/store/contention_tests.rs` hold a genuine SQLite `BEGIN IMMEDIATE` write lock using a second connection. The same Store and transaction closure that the application uses attempt an update. A short connection timeout makes the condition fast and deterministic without stalling Windows for five seconds.

1. **Transient lock:** the other connection holds the writer lock for 220 ms; the normal operation encounters `SQLITE_BUSY` after 10 ms. Before the correction the update was lost and the test failed. After the correction, bounded retries obtain the writer and persist exactly one intended update.
2. **Persistent lock:** the second writer remains active for 650 ms; all bounded attempts fail, no partial update is committed, and the user's next operation after unlock succeeds.

The initial test-only formatting run is not counted as a behavioral reproduction. On unchanged code, the formatted failure-first check on Windows 2025 produced **194 passing Rust tests and one failing targeted assertion**: `transient_writer_busy_at_begin_does_not_discard_a_user_change`. See https://github.com/TechWisard/desktop-pilot/actions/runs/40405269238 . Windows 2022's separate outcome is recorded in that run.

## Narrow implementation and boundaries

Only the **first acquisition** of a top-level `BEGIN IMMEDIATE` is retried, at most three attempts, with a 125 ms pause after retriable busy failures. The callback containing reads, changes and event writes is **never** replayed. Nested savepoints, SQL/schema verification, rollback, user data, paths, preferences and history are unchanged. `SQLITE_LOCKED`, non-SQLite errors and persistent `SQLITE_BUSY` are not hidden or endlessly retried. Under normal conditions no delay is added.

This is a bounded reliability improvement, not a guarantee against an indefinitely locked or damaged database. Existing full Rust, native Windows integration, UI model tests, Clippy, formatting and release guards must all pass on the final candidate and the eventual merged release. No new dependencies, permissions, UI controls, telemetry or external services were introduced.

## Unverified and next scenarios

No physical Windows 11 Explorer session, real-user productivity experiment or full extended stress run of three concurrent processes was performed. Continue real long-duration project changes, initial time-to-value, disappearing profile-drive recovery and GUI scaling studies. Keep the test evidence separate from claims of measured human benefit.
