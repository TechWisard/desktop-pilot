# Concurrent cold-start WAL transition — 2026-11-05

## Verified observation

An unchanged strict Windows 2025 CI runner encountered a concrete failure in the existing production-path test `store::tests::concurrent_first_open_preserves_a_complete_profile`. Four threads simultaneously opened the same fresh profile; one `Store::open` returned `SQLITE_BUSY / database is locked` despite its five-second busy timeout. The run completed with 398 Rust tests passing and this one failing: https://github.com/TechWisard/desktop-pilot/actions/runs/42029332596. This failure surfaced while testing the separate draft organizer PR #194, which did not modify `Store::open`.

The source includes an atomic first-time schema initialization followed by `PRAGMA journal_mode=WAL` on each new connection. A simultaneous first open can contend with the journal-mode transition even when the schema is valid. SQLite documents the WAL transition and the possibility of SQLITE_BUSY: https://www.sqlite.org/wal.html and https://www.sqlite.org/pragma.html#pragma_journal_mode. The exact statement that returned SQLITE_BUSY is **not identified by the existing test output**; the WAL-transition contention is a grounded hypothesis, not proven causation.

## Correctness-preserving change

The idempotent `PRAGMA journal_mode=WAL` step now retries only `SQLITE_BUSY` a bounded number of times, without retrying arbitrary database writes or replaying application operations. A failed WAL request still fails opening; foreign-schema validation, existing profile data and the atomic first-create transaction remain unchanged. Foreign-key checks are still enabled.

The existing native test is strengthened from one round of four simultaneous opens to five independent rounds using fresh profile paths. Every worker must open its actual production Store, persist its own record, and all records must remain readable on a new connection.

This is a reliability/correctness check on synthetic concurrent starts, not observed first-user latency or a claim about reducing real startup times. Additional prolonged stress, actual Windows 11 cold start and network/cloud state remain unverified.

## Acceptance

Run standard locked Rust test, format, strict Clippy, build, README, release guards and native integration on both public Windows 2022 and 2025 runners. Confirm results for the exact SHA before integration into `main`. If failure persists, preserve logs and investigate schema verification and initial writer contention separately; do not weaken the test.
