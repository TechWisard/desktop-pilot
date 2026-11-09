# Bound one-day bursts of short returns without discounting repeated days — 2026-11-09

## Evidence and failure reproduction

The earlier sustained-work correction ([research](2026-11-09-sustained-work-vs-brief-revisits.md)) handles six brief returns, but the production `rank_with_owned` still grants a 12-session bonus in one day. With two otherwise identical eligible apps and a single observed day, twelve verified 10-second visits (120 seconds total) produce `2*12 + 0.4*ln(1+120/60) = 24.4394`, while one 3600-second focus session receives `2 + 0.4*ln(61) + 3300/300 = 14.6443`. The short-check app therefore takes a vacant one-slot quick area even though the other has 30 times more foreground time. This is a deterministic source-derived mismatch with the purpose of the preceding correction, **not** a recorded customer incident.

The [Gajos et al. CHI 2008 controlled study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) reports usability benefits from both accurate and predictable adaptive interfaces; it does **not** prescribe numerical rankings. The [Czerwinski et al. task-interruption diary study](https://www.microsoft.com/en-us/research/publication/a-diary-study-of-task-switching-and-interruptions/) motivates treating context switching separately from sustained work. Reports about unexpected desktop layout changes in [Windows 11](https://www.reddit.com/r/Windows11/comments/1iy6cos/) reinforce avoiding unnecessary adaptation, but do not demonstrate that Desktop Pilot caused them.

## Narrow correction

Bound each calendar day's counted per-application sessions to six instead of twelve; keep real stored session, launch and foreground-second counters unchanged. Retain the existing one-hour bounded duration bonus, half-life decay, minimum new-placement evidence, independent-day counts, manual Always/Never rules, taskbar exclusions, replacement hysteresis and Undo. The twelve brief checks now score `12.4394` and the focused hour remains `14.6443` (same recency factor). Two days of six brief checks each still accumulate independently and can outrank a single focused day. This cap is a stated design trade-off, **not** an experimentally validated optimal weight: twelve deliberate short tasks may be valuable, and a user may explicitly pin their shortcut.

## Production-path regression and release gate

`a_same_day_burst_of_twelve_short_checks_cannot_beat_one_hour_of_focus` calls the real `rank` and `select_at` code for an empty slot, a week away, two separate active days, and an owned manually positioned essential app. It asserts eligibility, actual foreground seconds, recommendation, and precedence of explicit choice. Existing tests verify initial short-visit gates, background consent, duplicate launch profiles, offline recovery, source file integrity, Windows native integration, UI, and rollback. CI must pass on the exact proposed head before merging. Synthetic behavior comparisons are not measured human time saving.

This change belongs only to the unreleased integration branch (0.3.0 draft). Do not change published v0.2.128 or release the organizer until physical multi-monitor/DPI, live OneDrive, keyboard and interrupted-power acceptance is established. No new settings, telemetry, permissions, storage or background operations.
