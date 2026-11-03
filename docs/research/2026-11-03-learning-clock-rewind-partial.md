# Partial learning sessions after a major wall-clock rewind — 2026-11-03

## Evidence and decision

Continue the existing [clock-correction recovery](2026-10-26-clock-correction-recovery.md), [mixed-project continuity](2026-10-31-mixed-project-clock-recovery.md), and [adaptive-shortcut research](2026-10-26-adaptive-shortcuts.md). Microsoft's [system-time guidance](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) distinguishes adjustable clock time from elapsed time; the [CHI 2008 predictability/accuracy study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivates conservative, intelligible recommendations. Neither source establishes how often Desktop Pilot users correct their clocks or what session threshold is universally best.

**Confirmed code-path defect:** `learning::Progress::observe` detected any backwards timestamp and moved `last_active_at` to the earlier time, but kept an unfinished `session_seconds` accumulator. A saved four-minute incomplete session before a *large* backwards correction, followed by a one-minute confirmed visit at the corrected clock, therefore met the five-minute continuous-session threshold. On the default sessions-based learning mode, that could advance the learning gate and eventually allow an automatic shortcut earlier than intended. This is a synthetic failure mode inferred from production logic, not a documented user complaint.

## Reproduction through production components

The new `learning::tests::large_clock_rewind_does_not_complete_a_stale_partial_learning_session` test exercises `Progress::observe`, serialized `Progress` restored from the real `Store` SQLite profile, the production `ready` learning gate, `finish_if_ready`, and another actual SQLite save/read:

1. Accumulate 240 confirmed seconds. The session is not qualified.
2. Persist and restore the profile, then inject a timestamp more than 30 minutes behind `last_active_at`.
3. Add 60 confirmed seconds after the correction. **Before:** the old partial accumulator reached 300 seconds, counted one session and passed the one-session test gate. **After:** it holds only 60 seconds, zero sessions and an unready gate.
4. After an independent ordinary gap, 300 truly confirmed seconds earn one session; completion is persisted and survives reload.

The companion `short_clock_rewind_preserves_partial_progress_without_extra_sessions` checks two related boundaries: small backwards corrections keep an existing partial session, and a completed session is not counted twice after a larger rewind. The 30-minute boundary is the existing `SESSION_GAP`, not a new preference, threshold, database field or background timer.

These controlled observations use the actual production learning implementation and SQLite. They do **not** measure human productivity, recreate a physical Windows clock change, or prove the elapsed-time distinction for every sleep/restart sequence. Calendar-day totals intentionally remain cumulative: this change only separates *sessions* across a major discontinuity.

## User impact and limits

An anomalous clock change no longer joins two otherwise separate fragments into a false qualified session. Already completed learning remains complete. Confirmed short interruptions, manual Always/Never rules, desktop copies, rollback, privacy, and local-only storage remain unchanged. An unfinished four-minute session can be conservatively discarded across a major rewind; this prioritizes avoiding surprising automatic adaptation over the quickest possible first suggestion.

Run the unchanged Windows Server 2022 and 2025 checks: `cargo fmt --check`, all locked Rust tests, strict `cargo clippy --locked --all-targets -- -D warnings`, locked build, native integration checks and release guards. A successful CI run is a code-level acceptance result, not Windows 11 Explorer, real UI scaling, actual sleep/resume, or evidence of time saved.

## Next

Test major *forward* clock adjustments and project changes using monotonic foreground evidence without counting legitimate long breaks as adjustments. Continue UI first-use and accessibility checks on Windows 11 at 100%–200% DPI; keep the unrelated organizer draft PRs separate.
