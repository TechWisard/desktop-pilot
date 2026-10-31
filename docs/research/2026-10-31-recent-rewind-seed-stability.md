# Stable familiar shortcuts after a small Windows clock rewind — 2026-10-31

## User problem and evidence

A familiar, automatically managed shortcut can appear to lose all of its recorded historical priority if Windows adjusts its wall clock **back by several hours** after that shortcut's most recent score checkpoint. Previously, `model::rank` ignored the entire saved `seed` whenever `seed_at > now`; a different application's legitimately newer priority could then evict the familiar shortcut at an otherwise valid review. This is a source-confirmed logic defect, not a measured user incident.

Reusing the existing HCI survey in [adaptive shortcuts](2026-10-26-adaptive-shortcuts.md): Findlater and McGrenere (CHI 2004) and Gajos et al. (CHI 2008) motivate stability, predictability and retained manual control, but do **not** quantify Desktop Pilot's user productivity. Microsoft's [System Time documentation](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) describes adjustable Windows wall time. Existing [clock recovery research](2026-10-26-clock-correction-recovery.md) protected session checkpoints and automatic-review due dates but did not cover the `seed_at` ranking gate. No new search of already indexed literature was necessary.

## Failure-first real-path reproduction

The regression `model::cold_start_tests::a_recent_clock_rewind_must_not_evict_a_familiar_shortcut` invokes actual Windows `.url` parsing, the real SQLite `Store`, an initial `engine::apply` that places a managed desktop copy, and production `engine::preview` ranking/selection. It creates one previously trusted original (seed 100), a newer but weaker alternative (seed 40), then moves only the first application's checkpoint four hours ahead of the current time. This models a modest backwards clock adjustment without changing the machine clock.

On the unchanged runtime, the formatted test-only commit [`4d4b3dc`](https://github.com/TechWisard/desktop-pilot/commit/4d4b3dc55c23005dada897200a2d93b062252665) failed the *specific* unwanted-replacement assertion on both Windows Server 2022 and 2025 in [CI 41170859229](https://github.com/TechWisard/desktop-pilot/actions/runs/41170859229): **341 passed, 1 failed** on each. An earlier test authoring commit [`f60946d`](https://github.com/TechWisard/desktop-pilot/commit/f60946d92ebe6242551cf0733eadf3da58afaf55) failed formatting before executing any test; it is not counted as reproduction.

## Minimal correction and acceptance

For finite positive historical seeds whose checkpoint is **no more than 24 hours ahead** of the wall-clock snapshot, rank now uses zero elapsed aging (rather than discarding all saved history or extrapolating a future score). Older ordinary timestamps still decay identically. Seed timestamps more than one day ahead remain untrusted. There are no configuration/schema changes, filesystem changes, extra background work, network access or telemetry.

The same production-path regression must now show *zero* proposed additions/removals for the modest rewind (versus one added and one removed before the fix). It additionally checks that an explicit `Never` choice still overrides the retained historical score, that a more-than-three-day future checkpoint is not treated as recent evidence, and that source bytes, managed copies and ownership are not changed by preview.

Validate on the normal public Windows 2022 and 2025 CI with formatting, locked Rust suite, strict Clippy, build, native integration and publication guards, then verify the merged `main` SHA. Synthetic scenarios do not prove actual human time savings. Still outstanding: behavior after **larger** time corrections, actual Windows 11 at 200% display scaling, disconnected physical network roots, multiday project transitions and interactive task-level measurements.
