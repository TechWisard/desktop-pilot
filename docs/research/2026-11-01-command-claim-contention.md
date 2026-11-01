# Defer SQLite-contended command claims without dropping user actions — 2026-11-01

## Scope and evidence

This follows the existing recovery analysis in [stalled commands](2026-10-30-stalled-command-recovery.md), [terminal command outcomes](2026-10-30-terminal-command-outcomes.md), and [advisory writer lock recovery](2026-11-01-advisory-status-lock-recovery.md); the earlier literature and forum investigation is not repeated. SQLite's [result codes](https://www.sqlite.org/rescode.html#busy) distinguish transient BUSY/LOCKED from unexpected storage failures. The existing primary HCI work on [predictability and accuracy](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivates reliable user actions, but does not establish Desktop Pilot usage frequency or time saved.

**Source-confirmed control-flow problem:** in `engine::monitor`, both a requested rescan and a normal queued manual action used `store.claim_command(id)?`. The real store claims an action atomically inside `BEGIN IMMEDIATE`, with bounded retries and a SQLite busy timeout. If another writer still held the database, the error propagated out of the monitor loop and ended foreground observation. The original command remained `pending`; after a restart, `recover_commands` could cancel it. This is a demonstrated source-level risk, not a reported user incident.

## Narrow correction

Use the real `Store::claim_command` for both dispatch paths. Only when it fails with SQLite `DatabaseBusy` or `DatabaseLocked`, leave the command pending and continue the monitor poll. The next poll can claim it after the writer releases. Claiming is the **pre-execution** step: no shell action, placement, policy update, or rescan has started, so deferral cannot repeat a partially executed command. Corrupt schema, permissions, and non-SQLite errors still propagate; cancellation, execution and completion semantics are unchanged.

## Real SQLite regression

`engine::command_claim_contention_tests` submits both a `rescan` and a manual `policy:fixture:Always` command to the actual Store, holds `BEGIN IMMEDIATE` on a second WAL connection, and verifies neither command becomes `running` or disappears. After rollback the exact same commands are claimed once, subsequent claims do nothing, and two terminal completions emit exactly two audit events. A separate test drops the commands table in an isolated test profile to require non-contention errors to remain visible. No real application or desktop files are changed.

These are synthetic interleavings, **not** measurements of human task time or observed Windows 11 contention. The rest of the monitor still treats non-advisory persistence failures as fatal; this correction intentionally covers only a safe-before-execution transition.

## Acceptance and remaining work

Require strict cargo formatting, locked tests, warnings-denied Clippy, build, native Windows integration and publication guards on both standard Windows runners. Do not merge until CI is green on the exact branch commit. Physical Windows 11/UI tests, minutes-long disk stalls, cloud desktop root disconnect/reconnect, and canceled-command UX remain open acceptance scenarios. The draft organizer PRs #98/#101 are separate.
