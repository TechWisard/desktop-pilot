# A queued review after the user extends its cadence — 2026-10-31

## User-visible risk and evidence

A quiet automatic desktop review can pass its seven-day scheduling check and then wait behind another operation. During that wait, the user may save a longer, 30-day review interval to avoid premature shortcut movement. The existing queued-review correction in [paused review permission](2026-10-31-paused-review-commit-race.md) rereads collection and automatic permissions under the placement mutex, but did not reread the *due date* or the updated learning gate. Code inspection therefore identified a reproducible race hypothesis, **not a measured real-user incident**.

Prior user-interface evidence is retained in [adaptive shortcuts](2026-10-26-adaptive-shortcuts.md): Findlater/McGrenere (CHI 2004) and Gajos et al. (CHI 2008) motivate predictable, user-controlled adaptation. These studies do **not** establish that Desktop Pilot users encounter this particular race.

## Reproduction on real program paths

The regression `model::cold_start_tests::extending_review_interval_while_a_review_is_queued_prevents_early_replacement` builds a one-slot desktop with two inert native Windows `.url` shortcuts, actual SQLite Store and production ranking/preview, placement and rollback logic. A review that was due eight days after its previous run under a seven-day interval is paused at the existing deterministic boundary inside `automatic_review_with_boundary`. A second saved SQLite config extends the interval to 30 days before the worker proceeds.

The scenario requires the worker to cancel the obsolete review, leave the managed shortcut in place, preserve both source files, leave `last_review` unchanged and not create extra desktop items. Resetting the interval to seven days must allow the normal review; Undo must restore the original copy. No actual application is launched.

## Minimal correction

The existing named Windows placement mutex remains the serialization boundary for automatic placement and Settings saves. Inside that mutex, the reviewer now rereads the *current* state, schedule, learning progress, collection permission and shutdown/disabled controls, and reevaluates eligibility before entering the existing `apply_locked` transaction. The ordinary manual Apply path and its rollback are unchanged; no extra worker, background polling, telemetry, dependencies, source rewriting or release-guard changes.

This also avoids redundant work if another worker completed the same due review while this worker was queued. A changed learning gate does not result in a spurious placement attempt. The scheduling formula, including the prior system-clock correction exception, is factored into a single helper shared by initial and final eligibility checks.

## Validation boundaries

The independent [failure-first PR #88](https://github.com/TechWisard/desktop-pilot/pull/88) retained the regression without the fix. [Windows 2022/2025 run 41157040304](https://github.com/TechWisard/desktop-pilot/actions/runs/41157040304) failed this precise test on both Windows runners (one: **339 passed, 1 failed**) with the expected premature-placement assertion. This establishes the previously suspected defect in the actual application path, not its frequency among real users.

The correction [`4b0c864`](https://github.com/TechWisard/desktop-pilot/commit/4b0c8641d984a16c27d6c802d9f68a76d703623b) was merged through [PR #87](https://github.com/TechWisard/desktop-pilot/pull/87). Both [pull-request CI 41156976118](https://github.com/TechWisard/desktop-pilot/actions/runs/41156976118) and [push CI 41156972212](https://github.com/TechWisard/desktop-pilot/actions/runs/41156972212) passed on Windows 2022/2025 with Rust tests, formatting, strict Clippy, build, native integration and publication guards.

These are synthetic scheduling events against real implementation paths, not a human task-time study. Real Windows 11 high-DPI input latency, network-source disruptions and multiday project changes remain unverified.
