# Clock correction recovery — 2026-10-26

## Why this case matters

An unexpectedly corrected wall clock can make a locally stored “last seen” or “last review” timestamp look like an event in the future. When the program restarts after the clock is corrected, this is not user inactivity, and a user-selected desktop position is not permission to rearrange icons.

**Primary source:** Microsoft explicitly warns that Windows system time is adjusted both forwards and backwards, and should not be used to measure elapsed time: https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time . Rust's `Instant` is intended for elapsed-time measurement and uses `QueryPerformanceCounter` on Windows, though suspend and underlying clock behavior are platform dependent: https://doc.rust-lang.org/std/time/struct.Instant.html . These documents establish the wall-clock hazard; they do **not** establish the frequency or impact on Desktop Pilot users.

**Existing project research reused:** Findlater and McGrenere (CHI 2004) on stability and control in static/adaptive/adaptable menus, Gajos et al. (CHI 2008) on predictability and accuracy, and Findlater et al. (CHI 2009) on adaptation without spatial rearrangement are already linked in [adaptive-shortcuts.md](2026-10-26-adaptive-shortcuts.md). They motivate not moving manually protected files. No new user study was performed.

## Confirmed in production code, with failure-first tests

The code that writes usage events already uses monotonic elapsed time to determine whether foreground evidence is countable. It also stores UTC wall-clock timestamps per application so that a short switch or worker restart does not create extra sessions. Previously the first confirmed visit after a two-hour backward clock correction could be recorded with **zero new sessions**: the stored last-use timestamp was still two hours ahead. Further observations replaced that checkpoint, so the issue affected the first newly confirmed session, not a permanent suspension.

The automatic-review scheduler previously compared the current wall time to `last_review + interval_days`. When `last_review` was fourteen days in the future and interval was seven days, an enabled, quiet and already initialized installation had no review until the wall clock passed that future threshold. No program setting reflected this apparent inactivity.

Failure-first Windows runs on the test branch:
- [Initial historical session reproduction](https://github.com/TechWisard/desktop-pilot/actions/runs/40362314156)
- [Review scheduling reproduction before the production correction](https://github.com/TechWisard/desktop-pilot/actions/runs/40363233853)

**Implementation:** A confirmed first visit counts as a new session when the last persisted checkpoint is at least 20 minutes in either direction; adjustments shorter than that do not manufacture a session. Automatic review treats a stored review date more than 24 hours in the future as due, without bypassing automatic-review opt-in, learning readiness, the quiet-desktop requirement or initial preview policy. After a successful review, the actual timestamp is saved; an immediate duplicate review is not due. The normal configured interval is unchanged.

The 20-minute and 24-hour tolerance choices are **engineering thresholds**, not empirically established user behavior; no system clock is modified by the program.

## Reproducible scenarios (actual code paths)

| Sequence | Previous behavior | Corrected behavior |
| --- | --- | --- |
| Persisted session checkpoint +2 hours; worker restarts; first confirmed 10-second visit | Usage time recorded, 0 new sessions | Usage time recorded, 1 new session; checkpoint updated in the same SQLite transaction |
| Same app 30 seconds later, then 1,230 seconds later | Subsequent visits allowed but first session lost | No duplicate at 30 seconds, a new session after the normal gap; 25 total confirmed seconds in fixture |
| Checkpoint only 30 seconds in the future | No new session | Still no new session |
| Automatic review checkpoint +14 days, interval seven days | No quiet review | One review, then no immediate second review |
| Same future review while the desktop is busy | No review | Still no review |
| Previously managed `Always` shortcut and review checkpoint +21 days; toggle automatic review off, then on; busy, then quiet | Existing scheduler unable to trigger normally | No review while opted out/busy; one normal quiet review after opt-in; exact shortcut file bytes, path, filesystem identity and selection remain unchanged |

The tests invoke production `record_usage`, Store/SQLite, `automatic_review` and `apply` with isolated Windows fixture desktop directories. They are not copies of the business logic and do not access the user's desktop. All elapsed durations, event counts and movement counts above are **synthetic checks**; no actual user productivity gains or physical Windows Explorer behavior are claimed.

## Verification and remaining work

At commit `a7d09e587ace1800b8a2163a370fda5cb20b98d2`, the [Windows 2022/2025 run](https://github.com/TechWisard/desktop-pilot/actions/runs/40363813409) passed 177 Rust tests, 28 native integration checks, nine publication guards, formatting, warnings-denied Clippy and a debug build on each Windows runner. A follow-up regression checks the manual-shortcut and opt-in sequence; **its updated branch CI must be checked separately**, rather than inferred from the earlier run.

Unverified: changing the Windows 11 clock interactively, daylight-saving or timezone transitions, a multiday sleep/resume with genuinely different monotonic timing, physical Explorer position restoration at 200% display scaling, and effects on human task time. Continue longer project/habit adaptation, recovery of missing custom-profile drives, and a controlled UX study separately. Existing user files and settings remain untouched; no telemetry or background network traffic is introduced.
