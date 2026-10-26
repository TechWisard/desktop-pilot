# Session checkpoints after a forward wall-clock correction (2026-10-27)

## Baseline and external evidence

Continuation of the published 0.2.14 forward-time cleanup protection, not a new ranking algorithm. Starting main commit: `b01c643a6f27656a8ae90b0bbe45e4143ef257ba`. The previous investigation is [forward-clock-retention.md](2026-10-26-forward-clock-retention.md), including the Microsoft System Time reference: https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time . Windows system time can be corrected both forward and backward; elapsed monotonic time is separate. Research about predictable adaptive menus and concrete launcher users' reports is already indexed in [adaptive-shortcuts.md](2026-10-26-adaptive-shortcuts.md) and [alias-recovery.md](2026-10-26-alias-recovery.md). These sources motivate preserving continuity but are not product-specific empirical evidence.

## Confirmed production defect

The 0.2.14 SQLite `Store::housekeeping` correctly defers time-based deletion after a sudden forward clock jump. However the foreground `engine::record_usage` method independently cloned `app_session_times`, then called `retain` using `time - last < 90 days`. Recording a *real* visit in an unrelated application at a temporarily incorrect future system time therefore deleted recent checkpoints for other apps from both the monitor cache and SQLite. After the clock was corrected back, a short return to one of those apps could be miscounted as a new session, inflating its ranking.

This is a source-confirmed discrepancy between two real production write paths; a physical Windows clock was not changed.

## Failure-first verification

The test-only branch initially added the real `record_usage` regression and an ordinary `Store::housekeeping` control. After formatting was corrected, the unchanged Windows 2022/2025 CI run https://github.com/TechWisard/desktop-pilot/actions/runs/40401830909 had **190 passing Rust tests and one failed assertion on both OS runners**: `rare-project was pruned from worker memory`. The ordinary 95-day retention boundary passed. The earlier formatting-only CI run is *not* counted as behavioral reproduction.

The fixture records two apps' last sessions 30 and 60 days ago, observes a third app during a simulated +120-day clock discontinuity and then another visit after correction. The expected change is from two unintended checkpoint deletions to zero. An additional regression checks that a visit only 15 minutes after a previous real visit is not incorrectly counted as another session, while a third verifies that normal 90-day cleanup still updates the worker cache. The tests exercise real SQLite, the production usage writer and housekeeping; only the wall-clock argument to the existing writer is controlled. Synthetic row counts are not real human productivity measurements.

## Narrow correction

The foreground writer now only persists an acknowledged visit and session checkpoint. It does **not** make independent, irreversible age-based decisions about unrelated apps. The existing seven-day guarded `Store::housekeeping` remains the sole owner of time-based deletion, and after housekeeping the monitor reloads the authoritative persisted session map, avoiding reintroduction of expired in-memory entries.

No new settings, network requests, external service, schema or dependency. Original shortcuts, user-defined Always/Never rules, placement, undo, history and local-only processing are unchanged.

## Acceptance and remaining work

Run the final branch and main CI on both existing Windows runners: `cargo fmt --check`, `cargo test --locked`, strict Clippy, debug build, native shortcut/undo checks and release guards. Publish only after successful checks and release artifact verification. Compare normal 90-day expiration with clock correction and confirm no performance regressions.

Still unverified: physical Windows 11 clock adjustments, real sleep/hibernation over months, actual user-perceived impact, and the wider project-switch/time-to-first-value questions. A future-system-time session for the app actively used during the jump remains a recorded observation; the correction intentionally protects other apps' already earned checkpoints without silently rewriting dates.
