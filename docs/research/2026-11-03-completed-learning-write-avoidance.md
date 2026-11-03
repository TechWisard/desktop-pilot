# Avoid SQLite writer contention after learning completes — 2026-11-03

## User scenario and evidence

After the configured seven learning sessions, Desktop Pilot should keep observing permitted activity and maintaining usage recommendations without continually taking an exclusive database writer merely to discover that the initial-learning gate is already complete. A user may meanwhile be saving settings, importing many shortcuts, or backing up their local profile. Competing for a writer in that steady state can delay foreground sampling and waste power.

**Confirmed from production source:** the monitor calls `record_learning_if_permitted` on positive confirmed foreground ticks while learning is enabled, including after the saved `Progress.completed_at` is nonzero. `record_learning_at` called `store.transaction` before reading that completion marker. `Store::transaction` starts a SQLite IMMEDIATE writer transaction, with bounded retries. The inner check returned false without writing, but the writer lock was still acquired. SQLite documents that WAL allows readers and writers concurrently, but only one writer at a time: https://www.sqlite.org/wal.html . The official `PRAGMA query_only` reference documents that writes fail with SQLITE_READONLY: https://sqlite.org/pragma.html#pragma_query_only .

This is a verified code-path inefficiency and a plausible contention risk, **not** a measured real-user slowdown, CPU reduction, or battery-life improvement. Preserve previous adaptive-interface research instead of repeating a human-factors search: [adaptive shortcuts](2026-10-26-adaptive-shortcuts.md), [writer contention](2026-10-27-sqlite-writer-contention.md).

## Production-path correction

On each learning tick, read only the durable `Progress.completed_at` marker. If completed, return without acquiring the SQLite writer. If not, keep the existing `collection_permission`, completion recheck, sample observation, and save inside the same transaction. A concurrent explicit reset may cause an in-flight sample to be skipped; it cannot cause pre-reset samples to be replayed or count learning without permission. No schema, UI, startup, telemetry, shortcut, or user-rule changes.

## Deterministic regression

`src/engine/learning_clock_tests.rs::completed_learning_never_needs_a_sqlite_writer_even_during_a_foreground_sample` uses the **real Store SQLite database** and production learning entry point, with a completed profile persisted and reopened through the same connection. It sets SQLite's `PRAGMA query_only=ON` so a hidden IMMEDIATE writer acquisition is rejected deterministically, not inferred from a wall-time threshold:

1. Three further confirmed samples must return `Ok(false)` without a write transaction or altering the completed state. Before the correction, `record_learning_at` attempted the writer and returned a read-only error.
2. Reset learning deliberately. The unfinished profile must still reject a write under `query_only`; it may not bypass transactional safeguards.
3. Return the connection to writable mode: confirmed learning must advance the genuine stored session/day counters.

This is a functional/locking regression, not a benchmark. Also retain the midnight, pause, startup, session, and SQLite-lock tests. Strict cargo formatting, all tests, Clippy, build, native Windows integration and release safeguards are required before merging. Verify Windows 11 interactive behavior separately.

## Remaining

- Measure actual foreground throughput and database lock contention with extended realistic desktop activity.
- Test concurrent reset/setting writes and more complex background writer schedules.
- The unrelated desktop-organization PRs #98 and #101 remain draft; do not include them in this release.
