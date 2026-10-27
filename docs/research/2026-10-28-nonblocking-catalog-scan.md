# Keep manual commands responsive during slow catalog scans — 2026-10-28

## Starting point and evidence

Continue from published 0.2.27, commit `041cf1d166b9d1654522f6469a0925f86b37f926`. No open pull requests at the start of this investigation. The persisted SQLite catalog, previous no-op rescan optimizations, source-identity checks, manual rules and Undo remain in place. The ongoing cold-start acceptance work is issue [#22](https://github.com/TechWisard/desktop-pilot/issues/22).

Earlier primary adaptive-interface studies, particularly Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004, https://doi.org/10.1145/985692.985704), motivate spatial predictability and user control. They do not measure this application's productivity. A [Flow Launcher issue #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), opened 2026-05-21, reported a large custom shortcut library unavailable for about 20 minutes after restart; the reporter subsequently indicated that updating plugins apparently fixed the problem, and the issue was closed 2026-06-08. This is an independent anecdote about a different application, not evidence of a 20-minute Desktop Pilot delay.

Microsoft's [COM initialization guidance](https://learn.microsoft.com/en-us/windows/win32/learnwin32/initializing-the-com-library) requires each thread using COM interfaces to initialize COM separately. The isolated worker therefore initializes its own COM apartment; it does not share COM objects or a SQLite connection across threads. This is an engineering requirement, not a benchmark.

## Actual source defect and failure-first reproduction

At 0.2.27, the production `engine::monitor` event loop checks its command queue, then performs synchronous `refresh_apps` during startup/periodic maintenance. Reading a sluggish or disconnected shortcut source can block that same thread before its next queue check, heartbeat or shutdown check. The saved catalog remains readable in Settings, but explicit Always/Never or a shutdown can wait on an unrelated source.

`src/engine/blocked_scan_tests.rs` holds the **real** `refresh_apps` function at a test-only filesystem-I/O boundary. It uses the real Windows .url parser, independent SQLite Store, monitor startup, actual policy command queue, heartbeat and shutdown. On the original production code plus the new regression, [failure-first CI 40581043939](https://github.com/TechWisard/desktop-pilot/actions/runs/40581043939) passed 246 existing tests and failed the one new blocked-scan command-timing regression on both Windows Server 2022 and 2025. An earlier authoring run 40580538576 failed formatting and is *not* claimed as behavioral evidence.

The holding gate reproduces a blocked I/O boundary deterministically, not a measured network share stall. The 19-second hold is an injected wait and not a human task-time result.

## Narrow production correction

A single outstanding catalog scan now runs in a separate worker with its own COM initialization and SQLite Store connection. The existing monitor still polls input, records confirmed usage, maintains its heartbeat, processes commands and handles shutdown while the scanner reads disk. A requested Rescan attaches to the in-progress scan rather than creating unbounded duplicate workers.

The scan has a shared cancellation flag. If the owning monitor shuts down during a blocked Windows read, its worker cannot commit late results after the read returns. The existing transaction-time checks for unfinished placement, source changes and manual policies still run inside the actual catalog transaction. Automatic layout reconciliation/review continues only after a completed verified scan; a failed scan reports an error and is not retried every two seconds.

No new user setting, network access, dependency, telemetry, file rewrite, scoring change or database schema modification.

## Reproducible acceptance test and limits

Against the corrected production path the fixture requires:
1. A queued explicit `Never` decision completes while the filesystem scanner remains blocked, before its seven-second fixture deadline.
2. The saved heartbeat advances by at least 15 seconds during the same 19-second blocked scan.
3. Shutdown completes within a five-second fixture window without waiting for the blocked source.
4. A user-edited original remains byte-for-byte intact; cancellation prevents an abandoned scan from committing a stale target.
5. Manual rule persists; all other ranking, shortcut recovery, UI scale, Undo and native integration regressions still pass.

Run unchanged Windows CI on both runners: README validation, nine publication safeguards, `cargo fmt --check`, `cargo test --locked`, warnings-denied `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked` and 28 native checks. The initial corrected branch 40581915716 had 247 passing Rust tests but was not releasable because strict Clippy found an ignored nested test Result; follow-up test code asserts both monitor result levels rather than suppressing the warning. [Corrected branch checks](https://github.com/TechWisard/desktop-pilot/actions/runs/40583641596) must pass independently before release.

This only proves deterministic responsiveness and file safety on hosted Windows tests; it does not measure real Windows 11 startup/search/input latency, battery life or human time saved. The scan itself can still be delayed by a real network I/O wait. Remaining: 100/1,000/5,000 saved catalog startup timing at 100%/200% DPI, queued scan error recovery, a fully inaccessible profile disk, cloud sync clients, extended project/habit changes, false placement counts and physical Windows 11 Explorer behavior.
