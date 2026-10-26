# Idle monitor writes and background responsiveness (2026-10-27)

## Baseline and real-world motivation

Baseline is the released v0.2.15, commit `e09da093eb6e1ae61e20b262bdb54c812b05ac5c`. Its [release workflow](https://github.com/TechWisard/desktop-pilot/actions/runs/40407320657) succeeded; at the start of this investigation there were no open PRs. This continues rather than repeats the previous adaptive-layout, outage, session and SQLite investigations.

The SQLite project documents that WAL transactions append records and periodically require checkpointing: https://www.sqlite.org/wal.html . Microsoft notes that unnecessary background activity can increase battery usage, but gives **no measured effect for Desktop Pilot**: https://support.microsoft.com/en-us/windows/experience/performance-optimization/manage-background-activity-for-apps-in-windows . A Flow Launcher user described a stalled UI while computing a routine query: https://github.com/Flow-Launcher/Flow.Launcher/issues/4523 (2026-06-09). This is only analogous user testimony, not proof of this app's latency or energy impact. Earlier studies of predictable adaptive UIs remain cited in [adaptive-shortcuts.md](2026-10-26-adaptive-shortcuts.md).

## Source-confirmed inefficiency, not a productivity claim

In the prior production `engine::monitor`, an unconditional `Store::set("heartbeat")` committed a SQLite write after every approximately two-second poll, including in manual or paused-collection modes. That implies 1,800 writes in an otherwise quiet hour. But the Settings `health` function only treats the process as unresponsive when its saved heartbeat is more than **150 seconds** old, and startup requires the first saved positive heartbeat within 15 seconds. Two-second persistent writes therefore added avoidable database contention and WAL transactions without making the existing visible health status meaningfully more precise.

Engineering hypothesis: reducing **heartbeat-only** writes limits needless DB churn; it may improve idle behavior on some devices, but the real energy or latency effect is not yet measured. This is independent of actual use-event collection, which remains recorded at the original frequency for fidelity.

## Narrow change and explicit invariants

The worker now writes a heartbeat immediately on its first completed poll, then no more frequently than once per 15 seconds. Before a queued user command it forces a fresh heartbeat, just as before. After sleep/hibernation it publishes at most once rather than replaying missed polls. The scheduling clock uses Rust `Instant`, not an adjustable Windows wall clock. A failed write never advances the schedule.

No change to `ui::health`'s 150-second safety threshold, startup readiness, user command handling, settings, manual Always/Never, shortcut content or position, undo/history retention, telemetry or SQLite schema. No extra background thread or external service.

## Reproducible acceptance scenarios

The new tests in `src/engine/heartbeat_tests.rs` invoke the exact production heartbeat writer on an isolated real `Store` and count `SELECT total_changes()` writes, not a mock database. Over 1,800 synthetic two-second polls (60 minutes), the corrected code performs 225 SQLite heartbeat updates rather than the prior unconditional 1,800; expected relative reduction is 87.5% in this narrow counter only. Test 2 verifies immediate first heartbeat, a forced signal before a user command, boundary after 15 seconds, a single refresh after eight hours, and no accumulated late writes. Existing real worker-startup and Settings-health tests remain mandatory.

The before count is derived directly from the previous unconditional statement; the new tests do not execute an unmodified baseline worker for a real hour. Do not infer faster user tasks, SSD lifetime or battery savings from this fixture. Run the standard unchanged Windows 2022 and 2025 checks, native integration, Clippy, formatting and full release guards before merging. The release workflow must agree on commit, tag, version and asset hashes.

## Remaining higher-value investigations

Longitudinal changing-project recommendations, quantified false placements and unnecessary churn, first-use value, actual 200% Windows 11 GUI inspection, physical sleep/resume with cloud-synced folders, custom profile drive removal, and human task timings remain unverified. Prefer these over cosmetic UI controls or a Windows-version matrix.
