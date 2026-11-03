# Reject malformed calendar samples before learning clock correction — 2026-11-03

## Evidence and scope

This continues [partial-session rewind protection](2026-11-03-learning-clock-rewind-partial.md) and [clock-correction recovery](2026-10-26-clock-correction-recovery.md), without repeating general discovery. Microsoft's [Windows system-time documentation](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) distinguishes adjustable system timestamps from elapsed time. Gajos et al., [CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), found that accuracy and predictability both influence adaptive-interface satisfaction. A [Stardock Fences user's June 2024 account](https://forums.stardock.com/528482/icons-in-fences-get-rearranged-when-desktop-is-rebooted-or-refreshed) illustrates why preserving trusted manual state matters, but is **not** evidence of this bug in Desktop Pilot.

**Confirmed code-path defect, not a measured user incident:** `learning::Progress::observe` previously checked a backward time jump *before* rejecting malformed day input. A restored 240-second unfinished session could be erased by an invalid date string accompanied by an earlier timestamp, even though neither observation was eligible to earn activity. The next 60 legitimate seconds then could not complete the same five-minute session, unnecessarily delaying first recommendations. The actual foreground worker normally supplies a valid local day, so this is defensive preservation of persisted learning state across malformed replay/input, **not** a claim that Windows commonly emits invalid dates.

## Production-path regression and comparison

`learning::tests::malformed_day_during_clock_rewind_does_not_erase_saved_partial_session` opens a real SQLite `Store`, observes 240 seconds through the production `Progress::observe`, serializes the profile, reloads it, injects two malformed calendar samples with a backward timestamp, then submits a genuine 60-second return, completes the actual learning gate, persists and reloads it.

Expected before the fix (verified from the previous source branch, not a separately executed failing CI run): the first invalid backward sample resets `session_seconds` from 240 to 0, and the remaining real minute cannot complete the gate. After the correction, malformed samples leave `last_active_at`, the incomplete accumulator, calendar totals and persisted data untouched. The genuine minute completes exactly one session; prior fully completed learning and ordinary valid clock-correction handling remain unchanged.

No new setting, worker, dependency, telemetry, hidden file rewrite, or new UI control was introduced. The recorded comparison is a controlled state transition, not measured time saved by a person.

## Verification and remaining limits

Use normal CI on Windows Server 2022 and 2025: strict formatting, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, build, native integration fixtures and release guards. Keep release packaging separate unless the implementation passes these checks. A green CI validates this controlled scenario, not a physical Windows 11 clock change or live UI at 100–200% scaling.

Next: test real recovery after long sleep/clock changes and user-reported long-project switching with controlled monotonic foreground evidence. Keep draft desktop-organization PRs independent and do not infer customer productivity from tests.
