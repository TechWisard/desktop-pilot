# Bound one-day bursts of short returns without discounting repeated days — 2026-11-09

## Evidence and failure reproduction

The earlier sustained-work correction ([research](2026-11-09-sustained-work-vs-brief-revisits.md)) handles six brief returns, but the production `rank_with_owned` still grants a 12-session bonus in one day. With two otherwise identical eligible apps and a single observed day, twelve verified 10-second visits (120 seconds total) produce `2*12 + 0.4*ln(1+120/60) = 24.4394`, while one 3600-second focus session receives `2 + 0.4*ln(61) + 3300/300 = 14.6443`. The short-check app therefore takes a vacant one-slot quick area even though the other has 30 times more foreground time. This is a deterministic source-derived mismatch with the purpose of the preceding correction, **not** a recorded customer incident.

The [Gajos et al. CHI 2008 controlled study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) reports usability benefits from both accurate and predictable adaptive interfaces; it does **not** prescribe numerical rankings. The [Czerwinski et al. task-interruption diary study](https://www.microsoft.com/en-us/research/publication/a-diary-study-of-task-switching-and-interruptions/) motivates treating context switching separately from sustained work. Reports about unexpected desktop layout changes in [Windows 11](https://www.reddit.com/r/Windows11/comments/1iy6cos/) reinforce avoiding unnecessary adaptation, but do not demonstrate that Desktop Pilot caused them.

## Narrow correction

Count the first six sessions in a day at full weight, then gradually restore up to six further session credits as **total verified foreground work increases from five minutes (300 seconds) to ten minutes (600 seconds)**. Below five minutes, six is the session cap; at ten minutes, the original twelve-session cap applies. The interpolation is continuous at both boundaries. Actual recorded sessions, launches and seconds remain unchanged. Retain the existing one-hour duration bonus, recency decay, minimum first-placement evidence, independent-day counts, manual Always/Never, taskbar exclusions, replacement hysteresis and Undo.

The twelve 10-second checks still score `12.4394` vs `14.6443` for the focused hour (same recency factor). Two separate days of six short checks still accumulate independently. A day of twelve genuine work sessions with 600 seconds of confirmed foreground activity receives its prior score. This is a design choice, **not** an experimentally validated optimal weighting.

## Regression discovered and corrected during CI

The first simple six-session cap failed the **existing** production SQLite + engine review scenario `changing_projects_after_a_gap_keeps_the_rare_manual_shortcut_and_undo_is_respected` on Windows 2022/2025 (branch run [42593492473](https://github.com/TechWisard/desktop-pilot/actions/runs/42593492473) and PR run [42593500982](https://github.com/TechWisard/desktop-pilot/actions/runs/42593500982)): four days of twelve 600-second project sessions could no longer beat an older project with a saved score of 50. This is real regression coverage of project switching, and its assertion must not be weakened. The refined, foreground-time-weighted rule retains the previous twelve-session contribution for 600-second days while suppressing it for 120-second bursts. The existing indexed-history exact-score regression likewise retains the old expected score for a 7,200-second day. Full CI of the refined rule must pass independently before integration.

## Production-path regression and release gate

`a_same_day_burst_of_twelve_short_checks_cannot_beat_one_hour_of_focus` calls the real `rank` and `select_at` code for an empty slot, a week away, two separate active days, and an owned manually positioned essential app. It asserts eligibility, actual foreground seconds, recommendation, and precedence of explicit choice. Existing tests verify initial short-visit gates, background consent, duplicate launch profiles, offline recovery, source file integrity, Windows native integration, UI, and rollback. CI must pass on the exact proposed head before merging. Synthetic behavior comparisons are not measured human time saving.

This change belongs only to the unreleased integration branch (0.3.0 draft). Do not change published v0.2.128 or release the organizer until physical multi-monitor/DPI, live OneDrive, keyboard and interrupted-power acceptance is established. No new settings, telemetry, permissions, storage or background operations.
