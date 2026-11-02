# Future saved learning days after Windows clock correction — 2026-11-02

## Evidence and status

This continues [the original adaptation research](2026-10-26-adaptive-shortcuts.md), [forward-clock retention work](2026-10-26-forward-clock-retention.md), and [the immediately preceding malformed-day correction](2026-11-02-learning-invalid-saved-days.md). The [Microsoft system-time guidance](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) distinguishes adjustable wall clock from elapsed measurement. Findlater and McGrenere's [CHI 2004 comparison](https://doi.org/10.1145/985692.985704) motivates protecting user control and location stability, not a measured Desktop Pilot failure frequency. No new field observations or user-time savings are asserted.

**Confirmed source behavior at 4d73b333ecc5dd278feef418c139b44fcb9bc281:** `Progress::active_days` rejected malformed dates but accepted *valid future dates* after restoring older or edited JSON. In a 90-day learning configuration, 89 legitimate qualified days plus one future date 400 days ahead erroneously satisfied the learning gate. When a new genuine day began, the retention loop pruned that incomplete new day instead of the future, already-qualified row; repeated samples could not accumulate the final qualifying day.

## Production-path regression

`learning::tests::future_saved_day_does_not_unlock_learning_or_block_the_ninetieth_real_day` saves the actual `Progress` in the actual `Store` SQLite database, restores it, calls `active_days` / `ready`, and applies 150 regular two-second `observe` samples from the current local date. The 90-entry cap must retain 89 prior legitimate days, remove the 400-day future entry, and let the genuine new day reach 300 seconds. The completed state must survive SQLite persistence. A separate test verifies that an *already completed* learning gate remains complete without rewriting its saved history.

Before the correction, the first regression assertion would report **90** qualified days rather than **89**; the source-level behavior is directly reproducible. After the correction, expected state is **89 → 90** valid qualified days only after the genuine samples. These are synthetic fixture counts, not estimates of real human time saved.

The production check allows a one-local-day window to accommodate modest time-zone or clock corrections while preventing distant future evidence. The invalid and implausible rows are retained until the 90-entry bound requires pruning; merely opening a profile does not rewrite them. The existing completion checkpoint remains authoritative. No user files, manual rules, shortcuts, interface controls, dependencies or telemetry are changed.

## Acceptance and next work

Run unchanged public Windows 2022/2025 `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, native integration and publication guards on the exact PR head, then verify the merge `main`. A CI regression is not a physical Windows clock test or Windows 11 interactive acceptance. Keep unfinished organizer PRs #98 and #101 separate. Next priority: long mixed-project use and a disconnected catalog at cold start, with observed recommendation stability and change count measured on the real Store and selector.
