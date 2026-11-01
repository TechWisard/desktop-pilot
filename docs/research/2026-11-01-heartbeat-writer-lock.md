# Recover the foreground monitor after a transient heartbeat writer lock — 2026-11-01

## Context and independent evidence

Continue the existing [SQLite writer contention](2026-10-27-sqlite-writer-contention.md), [worker heartbeat](2026-10-27-worker-health-clock.md), and [command/blocked scan](2026-10-31-command-heartbeat-blocked-scan-validation.md) investigations. PR #112 on 2026-11-01 fixed the separate qualified usage and learning writers. This finding concerns the monitor's **periodic liveness write**, which remained unguarded.

[SQLite transaction semantics](https://www.sqlite.org/lang_transaction.html) and [SQLITE_BUSY codes](https://www.sqlite.org/rescode.html#busy) document writer conflicts. [Findlater/McGrenere CHI 2004](https://doi.org/10.1145/985692.985704) motivates predictable, controllable adaptation, not a measured speedup here. A [February 2026 Fences user report](https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop) describes unexpected desktop movement; that is an analogous reported frustration, **not** an observation of Desktop Pilot or evidence of its lock frequency.

## Source-confirmed failure path

`monitor` invokes `publish_heartbeat_if_due` every polling cycle (normally one write in 15 seconds, plus forced command signals). Before this change, that function directly propagated `Store::set("heartbeat")` errors with `?`. If an independent Settings/backup writer held `BEGIN IMMEDIATE` longer than the configured busy timeout, the monitor could exit during a heartbeat despite foreground tracking having no irrecoverable problem. This is a source-derived correctness defect; real-world occurrence is unknown.

## Narrow correction

Only a SQLite `BUSY` or `LOCKED` error on this advisory heartbeat write is treated as a dropped signal, not a worker failure. The last-published `Instant` is unchanged so the next poll tries again. Successful writes, intervals, cold-start readiness and forced command heartbeats remain unchanged. Corrupt schema, disk or other I/O failures still propagate. No changes to user shortcuts, settings, history, UI, schema, permissions, telemetry, or release policy.

## Production-path regression and expectations

`engine::heartbeat_tests::temporary_writer_lock_drops_only_the_heartbeat_and_recovers_on_next_tick` holds a real WAL writer lock in a second SQLite connection while invoking the actual `publish_heartbeat_if_due`. It asserts no error, no phantom timestamp or advanced checkpoint during the lock, then one persisted heartbeat immediately after unlock and normal cadence thereafter. Before correction, the first `unwrap()` would fail with SQLITE_BUSY. `non_lock_heartbeat_database_failure_is_not_hidden` deletes the `kv` table only from an isolated test profile and confirms that a non-lock failure still errors. The existing one-hour cadence and long-absence recovery tests remain active.

These checks simulate timed writer contention in the production Store/SQLite path; they do not run the Explorer UI, emulate a complete stalled process, demonstrate time savings, or guarantee survival of **other** writer failures elsewhere in the worker.

## Acceptance and follow-up

Require unchanged Windows 2022 and 2025 GitHub checks: locked Rust tests, formatting, strict Clippy, build, native integration, and release guards. Check exact commit and final `main` after merge. Follow-ups: inspect nonessential status writes under prolonged contention and command handling under competing writers, interactive Windows 11 and sleep/reconnect testing, 200% UI scaling, user study of time to first useful recommendation.
