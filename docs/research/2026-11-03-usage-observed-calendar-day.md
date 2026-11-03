# Foreground usage belongs to the observed calendar day — 2026-11-03

## Practical scenario and evidence

A person is working in an editor across local midnight while the local SQLite writer is briefly occupied, or a foreground sample waits to be committed after a pause. The duration and session checkpoint should describe the same observed moment as the calendar-day aggregate. Otherwise a legitimate late-night session appears as activity on the next morning's day and may distort a shortcut's independent-day evidence.

This extends [one-clock learning samples](2026-11-03-single-learning-clock-snapshot.md) rather than repeating earlier general adaptive-interface research. The [CHI 2008 study on predictability and accuracy](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivates predictable recommendations but does not establish the incidence of this date-boundary bug. Microsoft's [system-time documentation](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) describes the OS clocks; the reproduction itself is based on source inspection and native tests, not an observed user incident.

## Verified original inconsistency

`record_usage_inner` already accepted a sampled timestamp for the persisted `app_session_times` checkpoint. But the nested `Store::observe` called `Local::now()` independently when choosing a SQLite `days` bucket. A delayed writer or midnight crossing could split one event's time semantics.

The failure-first commit [3cf2ce1](https://github.com/TechWisard/desktop-pilot/commit/3cf2ce16b70b8c1d93111b9ac5401f0c558f0677) added `foreground_usage_keeps_sampled_local_day_across_midnight` to the existing engine/session retention tests. The real SQLite writer and session logic recorded **one incorrect row** of 24 seconds on today's date instead of two correct rows (12 seconds on yesterday and 12 seconds today). Windows Server 2025 [test-only CI](https://github.com/TechWisard/desktop-pilot/actions/runs/41681708893) ran formatting successfully and failed the exact new assertion: 404 tests passed, 1 failed. The new regression is not a standalone simplified ranker.

## Correction and acceptance

`Store::observe_at` uses the calendar day attached to the same confirmed local time snapshot as the session checkpoint. The monitoring writer captures its local timestamp once, outside retries, and passes it through the real SQLite transaction. For existing direct `Store::observe` callers a current snapshot is taken once. No second wall-clock read can reassign the day while the writer waits.

The regression replays a last valid local sample before midnight and the first available sample after midnight, including time zones where midnight may not exist due to a transition. It asserts exact daily date buckets, sessions, seconds and the final persisted per-app checkpoint. Related long-use and multiple-short-visit tests now use a deterministic local midday reference instead of a 1970 timestamp, because past synthetic timestamps should no longer be mislabeled as today's usage.

The change does not rewrite existing history, alter scoring thresholds, create external telemetry, move files or override manual layout choices. It does not validate real-world time savings or an arbitrary incorrectly set system clock.

## Next checks

Verify format, all locked Rust tests, strict Clippy, Windows build, native integration and release guards on the unmodified public CI. Follow with interactive Windows 11 tests at varying display scaling and with real SQLite contention around midnight, restoration after sleep, disconnected projects and longer genuine user activity sequences. Maintain organizer draft PRs #98 and #101 separately.
