# Observation pause between input sampling and SQLite commit — 2026-11-01

## Why this scenario matters

Continue the existing work in [short foreground visits](2026-10-26-short-foreground-visits.md), [pause permission](2026-10-31-paused-observation-write-boundary.md), and [adaptive shortcut research](2026-10-26-adaptive-shortcuts.md). The cited Findlater/McGrenere and Gajos human-interface studies motivate predictable, user-controlled adaptation, but do not measure this failure or prove time savings. A pause/resume race in the implementation is a source-derived hypothesis until exercised against the application writer.

The foreground sampler accumulates confirmed seconds locally and marks the **first** qualifying visit as recorded before its SQLite write. Separately, `record_usage_inner` correctly refuses writes when Settings pauses observation after the sampling snapshot. Before this change, the monitor ignored the returned `false` and kept the accumulator's already-advanced counter. If collection was re-enabled before the following monitor poll, later confirmed activity could be saved with `first=false`, leaving that application's actual session uncounted even though new work was observed. The same state could retain stale pre-pause evidence.

This is a deterministic ordering defect in production code, not a reported user incident. The relevant user scenario is a brief Settings pause during foreground work followed by rapid resumption, potentially across two projects. The existing SQLite permission boundary remains authoritative.

## Change

- Keep the normal busy/locked transaction retries and error propagation; make their return generic so the committed/refused Boolean is not thrown away.
- Route the production monitor's qualified sample through `persist_confirmed_usage` and the existing real SQLite writer.
- A refused sample clears the in-memory short-visit accumulator. The monitor also disables input sampling and resets focus confirmation; resumed activity needs fresh valid input rather than inheriting an older click.
- A successful transaction retains the existing cumulative-short-visit, 20-minute memory, 10-second threshold, independent session-gap and score rules. No new SQLite fields, telemetry, automatic placement policy, settings or background worker is introduced.

## Deterministic regression

`src/engine/paused_usage_tests.rs::paused_observation_cannot_consume_first_session_or_replay_old_activity` uses a real `Store`, actual `Config` persisted in SQLite, the production `UsageAccumulator`, and the same `persist_confirmed_usage` called by the monitor.

1. Six 2-second polls of editor input qualify 10 confirmed seconds; the first switching sample is ignored.
2. Settings pauses before saving. Verify no history/checkpoint write and no pending short-visit memory.
3. Resume, repeat independently confirmed activity, verify exactly 10 seconds and **one** editor session.
4. Switch to browser, pause its first qualifying sample, verify editor history remains untouched and no browser checkpoint appears.
5. Resume browser, verify exactly one browser session and no duplication of editor counters.

The prior path would leave the refused editor/browser windows populated. All timing/input events are synthetic; the database writer and usage accumulation are actual application code. The test is not a substitute for genuine window switching, OS accessibility checks or end-user timing studies.

## Verification gates and remaining work

Require strict Windows 2022/2025 CI (format, locked Rust tests, Clippy with warnings denied, locked build, native integration fixtures and release guards). Confirm exact branch SHA and post-merge main before merging; verify any formatting changes rather than disabling checks. Interactive Windows 11 testing including 100%/200% scaling, a manually toggled pause during foreground work and long idle/sleep sequences remain outstanding. No release should be published solely for a unit-level improvement without verifying the distributable.
