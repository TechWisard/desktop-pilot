# Pause collection: reject queued observation at the durable write boundary — 2026-10-31

## Context and observed control flow

The prior fixes [paused adaptation permission](2026-10-31-paused-adaptation-permission.md) and [queued-review pause](2026-10-31-paused-review-commit-race.md) protect **automatic placement** after the user pauses observation. This follow-up addresses a separate boundary: **recording history and learning progress**. When the user selects Observe/Adaptive and subsequently pauses observation, an already sampled foreground visit may remain queued in the background monitor after the Settings transaction has committed `config.collect=false`.

The previous `engine::monitor` loop used its polling-time `Config` snapshot and called `record_usage` with no persisted-permission recheck. Its learning-progress writer also used the polling-time configuration. Therefore neither write independently excluded observations sampled before the pause but committed after it. This is a source-confirmed race window, **not** a claim that a real user has experienced a privacy incident.

The existing [adaptive-interface research](2026-10-26-adaptive-shortcuts.md) motivates predictable controls, explicit manual authority and stability. The primary references there are Findlater & McGrenere (CHI 2004), Gajos et al. (CHI 2008) and Findlater et al. (CHI 2009). They are not studies of Desktop Pilot usage.

## Reproducible production-path regression

Test `engine::tests::paused_collection_rejects_in_flight_usage_and_learning_writes` exercises the **actual** SQLite WAL-backed `Store`, native-parsed inert shortcut fixture, session writer, learning-progress writer and a separate Settings connection. There is no simplified ranking model and no external application launch.

1. Begin with collection and learning explicitly enabled. Record 30 seconds of confirmed activity and a learning increment; snapshot day counters, session checkpoints, learned progress, shortcut bytes and ownership.
2. Have a second `Store` commit `config.collect=false`, representing a pause after the worker has sampled but before the writer finishes. Attempt a stale 600-second usage and learning update. The durable counters, checkpoints, in-memory session cache and learning progress must be unchanged.
3. Re-enable collection but commit `background_disabled=true`, representing Turn off. Both stale writes must still be refused.
4. Resume an explicitly permitted session and record 30 seconds, including the learning increment. The second confirmed session must appear, while the original shortcut remains byte-identical and no desktop copy is created.

**Behavioral bound:** exactly zero post-pause usage and learning commits in these controlled interleavings; valid fresh samples after resumption still commit. These are synthetic scheduling assertions on production code, not measured savings or captured real input.

## Implementation and transaction boundary

`engine::record_usage_inner` now checks persisted `shutdown`, `background_disabled` and `config.collect` inside the existing `Store::transaction` before updating activity and checkpoint records. `record_learning_if_permitted` performs an equivalent check inside its own transaction and also honors `learning_enabled`. The store uses SQLite `BEGIN IMMEDIATE` for the outer writer transaction, so a successful Settings pause and a subsequent sample commit cannot pass each other unnoticed. Rejected writes do not mutate the cached session times or carry a stale fractional learning sample to later ticks.

This does not change ranking weights, shortcut layout, user-authored rules, rollback, telemetry, network access or permissions. A write that finishes **before** a pause commits is legitimate earlier evidence; the guarantee begins at the saved permission boundary. An exceptionally long SQLite lock or file-system outage still requires separate recovery testing.

## Acceptance and remaining limitations

The source branch `fix/collection-pause-write-barrier-20261031` has [Windows checks 41187686090](https://github.com/TechWisard/desktop-pilot/actions/runs/41187686090): on **Windows Server 2022 and 2025**, formatting, locked Rust tests, warning-denied Clippy, debug build, native integration fixtures and release guards all completed successfully. The immediately previous run failed formatting and is **not** a failure-first behavior reproduction. A controlled red-run comparison against the earlier writer, if required, remains a separate exercise.

The verified code should be merged only after the combined PR candidate passes the same unchanged Windows checks; packaged release acceptance is separate. Physical Windows 11 at 200% scaling, real network interruptions, multiday changes in habits and measured time-to-first-use remain unverified.
