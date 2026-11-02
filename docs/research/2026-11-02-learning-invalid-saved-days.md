# Protect learning progress from malformed persisted calendar days — 2026-11-02

## Evidence and scope

Continue the [adaptive shortcut evidence](2026-10-26-adaptive-shortcuts.md) and [cold-start confidence review](2026-10-27-cold-start-confidence.md). The CHI studies cited there motivate dependable, comprehensible adaptation, but **do not** establish how often this malformed-profile scenario occurs or any time saving for users.

**Confirmed from the production code:** `learning::Progress::observe` validates the date of each new sample, while `Progress` itself deserializes legacy values with `serde(default)`. Previously `active_days` counted any persisted string with at least 300 seconds, including `2026-02-30` or `9999-invalid`. Under the 90-entry limit, an invalid but already-qualified string could also displace the next legitimate, incomplete day: retention searched only for entries below 300 seconds. This is a source-level defect in the existing learning gate, not a claim about real-user frequency.

## Reproducible sequence on the actual learning implementation

1. Restore a serialized `Progress` containing 89 distinct, valid, 300-second days and one malformed, 300-second key.
2. Configure the real `Config` for 90 qualifying days. Before this correction, `Progress::active_days` reports 90 and `ready` is true without the 90th real day.
3. After correction, `active_days` reports 89, `ready` is false, and the persisted dictionary is not rewritten merely by being read.
4. Advance another valid day using successive 2-second `Progress::observe` samples (the production accumulator), crossing the 300-second threshold. The bounded map must evict the invalid date, not discard each new incomplete daily sample. The 90th valid day then unlocks learning.
5. Separately check a malformed leap-day key, extra suffix and arbitrary text. A legitimately completed learning state stays completed rather than being reset by validation.

Unit tests are `learning::tests::invalid_saved_calendar_keys_never_unlock_learning_or_displace_real_days` and `learning::tests::invalid_saved_days_are_ignored_without_erasing_completed_learning`. The counterfactual follows directly from the previous `active_days` and pruning conditions; no empirical real-user timings are inferred.

## User-visible effect and limits

Incorrect old dates no longer prematurely authorize automatic changes or indefinitely prevent a real 90th day from accumulating. Valid progress and a completed learning gate are preserved; no telemetry, user-file modification, migration, new controls, or network calls are introduced.

Required release gates remain unchanged: strict formatting, Rust tests, Clippy `-D warnings`, build, Windows native integration and publication guards on supported CI. These tests cannot substitute for a physical Windows 11 UI, desktop/Undo or user study. Do not release before standard checks complete.

## Next

Extend long-duration simulation to mixed sessions, project changes and clock shifts with production `Store` evidence. Keep the separate organizer PRs #98 and #101 in draft pending their own Windows 11 and GUI acceptance. Preserve unique diagnostics before cleaning Actions artifacts/caches.
