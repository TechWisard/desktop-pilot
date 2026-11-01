# Keep observation alive when advisory status writes encounter SQLite contention — 2026-11-01

## Motivation and evidence

Continue [transient SQLite locks](2026-11-01-transient-sqlite-locks.md) and [heartbeat writer recovery](2026-11-01-heartbeat-writer-lock.md). Do not repeat their baseline research. SQLite documents [writer contention and BUSY/LOCKED codes](https://www.sqlite.org/rescode.html#busy); a second writer can block a transaction until the busy timeout expires. Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces* (CHI 2008), [primary source](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), find that reliability of adaptation and accurate behavior matter for satisfaction. A [Stardock Fences forum report from May 2026](https://forums.stardock.com/541585/desktop-icon-spacing-all-messed-up) describes unpredictable icon arrangements after changes in monitor state. This is user evidence about a different program, **not** a measured failure frequency in Desktop Pilot.

## Source-confirmed problem

The monitor already survived BUSY/LOCKED during foreground usage, learning, and 15-second heartbeats. Two less frequent advisory writers still propagated temporary lock errors with `?`: `publish_monitor_status_if_changed` during one-minute maintenance and `publish_hook_warning` during input-hook state transitions. Either could stop an otherwise healthy monitor while Settings, backup, or another SQLite connection held a writer transaction.

The hook transition had a second correctness risk: status and its warning event were previously independent writes. If the event failed after the status write, the UI could display a new warning without a matching event while the worker exited. No actual user's data-loss or time saving is inferred from the code inspection.

## Correction and explicit boundaries

- Publish a hook-state transition as one SQLite transaction containing the visible status and its warning event. Only after commit is the in-memory previous-warning marker updated.
- On SQLite BUSY/LOCKED, retain the pending transition and retry it on the next regular monitor poll. No status or warning event is fabricated during the lock.
- Defer a busy maintenance status update to the next scheduled maintenance review; keep the existing no-op suppression for unchanged text.
- Preserve explicit errors for corrupt tables, disk faults, and unexpected SQLite failures. Do not change the learning threshold, ranking, layout, shortcut files, manual choices, telemetry, or privacy model.

## Production-path synthetic regressions

The tests in `src/engine/heartbeat_tests.rs` use the production `Store`, actual SQLite WAL transactions and real publisher functions, with a second connection holding `BEGIN IMMEDIATE`.

1. A minute-end status transition under a writer lock must return a refused update without stopping the caller or falsely reporting success; after `ROLLBACK`, the next invocation persists exactly one update, and the unchanged status performs no extra write. Before this fix, the direct status setter returned SQLITE_BUSY.
2. A hook warning is attempted during a writer lock, then committed once on unlock; duplicate ticks do not duplicate its event, and warning recovery after another lock still clears the warning. Before this fix, the first locked status write returned an error.
3. Removing `kv` or `events` in isolated synthetic databases must still raise an error. A failed warning-event insert must roll back its status write atomically rather than produce partial visible state.

## Verification and remaining work

Require standard public Windows 2022/2025 tests, formatting, Clippy, build, native integration, and release guards at the exact commit before merge. These tests simulate short lock contention on real program logic; they **do not** demonstrate live Windows 11 UI behavior, multi-minute contention, external drive recovery, multi-monitor scaling, or measured user benefit. Other non-advisory writes remain deliberately fatal on errors. Continue ergonomic and longer-run studies separately.
