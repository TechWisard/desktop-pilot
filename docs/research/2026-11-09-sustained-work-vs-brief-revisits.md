# Sustained work and frequent short revisits — 2026-11-09

## Evidence and design question
The published [CHI 2008 controlled study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) by Gajos et al. reports that adaptation accuracy and predictability affect usability, with accuracy having a particularly strong effect on task performance. The [CHI 2004 task-interruption diary study](https://www.microsoft.com/en-us/research/publication/a-diary-study-of-task-switching-and-interruptions/) by Czerwinski, Horvitz and Wilhite documents substantial task switching and the difficulty of resuming complex work. Those are primary research sources about human behavior, **not** measurements of Desktop Pilot's ranking weights. No evidence shows that any specific numerical score is universally optimal.

## Verified source discrepancy
In the shipped local rank function, one day's six short return sessions totaling **60 seconds** produced raw evidence `2*6 + 0.4*ln(1+60/60) = 12.28`; one uninterrupted **3,600-second** work session produced `2*1 + 0.4*ln(1+3600/60) = 3.64`. Thus the frequently revisited app outranked an app used **60 times longer** on otherwise equal evidence. Existing selection then filled a vacant one-slot quick area with that short-visit app, despite the sustained session. The synthetic case exposes a design mismatch, not actual customer dissatisfaction.

## Bounded correction and regression
Reward additional *verified foreground time* only beyond 300 seconds, by `min(max(seconds-300,0),3300)/300` per observed day. Evidence is capped at an hour; the baseline session and logarithmic duration weights remain. The same synthetic values become **12.28 vs 14.64**, before common recency decay. A two-minute incidental visit receives no bonus and remains below the existing initial-placement gate. The model continues to respect independent-day confidence, explicit manual Always/Never, owned manual positions, taskbar exclusions, weekly review and replacement hysteresis. No settings, persisted schema, collection hook, telemetry or permissions changed.

`sustained_work_competes_with_brief_revisits_without_overriding_manual_choices` exercises the real `rank` and `select_at` functions across an initial ranking, vacant-slot selection, a one-week absence and an owned manual anchor. It checks that the short incidental visit is still ineligible and that manual positioning wins. This is a deterministic correctness and heuristic-comparison test, not a productivity benchmark. Other ranking and source/Undo tests must remain green.

## Release and validation boundary
Keep this on the existing unreleased integration branch until the exact proposed Windows 2022/2025/11 ARM build, full Rust tests, strict Clippy, native/Explorer/cloud/UI checks pass. Do not treat green hosted checks as physical multi-monitor/OneDrive/keyboard/power-interruption acceptance. Stable `main` and the published v0.2.128 remain untouched.
