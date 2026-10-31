# Queued automatic review and a newly committed observation pause — 2026-10-31

## Why this matters

In Desktop Pilot 0.2.70 an Adaptive profile that has already paused collection no longer starts an unattended review. Its saved preference can remain `automatic=true, collect=false` for later resumption. The prior control-flow analysis is in [paused adaptation permission](2026-10-31-paused-adaptation-permission.md); the previous human-interface evidence is in [adaptive shortcuts](2026-10-26-adaptive-shortcuts.md). This follow-up does not claim any measured benefit for real users.

One scheduling gap remained: a worker could read the old Adaptive permission, complete eligibility checks and then enter `engine::apply` *after* Settings had committed the observation pause. The earlier permission guard was too early. A second gap allowed an already active placement to overlap the Settings save, so the UI could acknowledge a pause before shortcut replacement finished.

## Reproduction against the real program

`model::cold_start_tests::pausing_while_a_due_review_is_queued_does_not_rearrange_shortcuts` uses the actual `Store` and SQLite/WAL file, two native-parsed inert Internet Shortcuts, the production ranking/preview and file-placement/Undo implementation, and a one-slot desktop. An overdue review prefers a new project shortcut to the currently managed old one. The production automatic-review path is suspended *exactly between the eligibility check and Apply*; a second `Store` connection commits `collect=false` while the old `automatic=true` preference remains; review then resumes.

**Before correction:** [Windows Actions 41154347159](https://github.com/TechWisard/desktop-pilot/actions/runs/41154347159) reached the real assertion on Windows Server 2022 and 2025. On each runner **336 tests passed, one failed**: the queued worker had still changed the desktop after the pause committed. An earlier formatting-only failure is not behavioral evidence. No original shortcut is launched.

A second scenario, `ui::save_disable_race_tests::paused_collection_save_waits_for_an_in_flight_placement_before_acknowledging_it`, holds the same real Windows `changes` named mutex used by the placement engine while a production asynchronous Settings save attempts to pause collection. The save must not acknowledge or persist the mode change until the placement lock has been released. It additionally checks source bytes, current owned shortcut and usage history.

## Correction and preservation

Scheduled review reacquires the real placement mutex and **rereads persisted collect, automatic, disabled and shutdown state under that mutex** immediately before invoking the unchanged placement implementation. Manual Apply still uses the same transaction/rollback path; the only new automatic branch is a cancelled queued review when permission was revoked.

The Settings worker waits for an existing placement to finish *before committing and acknowledging a mode edit*. It waits in the asynchronous save worker, not the visible UI thread. A bounded 12-second wait returns an explicit failure with settings unchanged if another placement cannot finish; there is no silent bypass or new background watcher. Consequently, a committed and successfully acknowledged pause cannot be overtaken by an automatic review that had previously observed the old mode.

No changes to ranking, learning weights, layout density, source shortcut files, rollback structure, autostart permissions, or private user data; no telemetry or new network dependencies. The normal workflow and release guards stay enabled.

## Validation and limits

For the corrected branch, run unchanged `cargo fmt --check`, `cargo test --locked`, strict `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, the Windows native integration fixtures and publication guards on **both** Windows Server 2022 and 2025. Verify actual green Actions results before merging. Preserve any failing evidence and do not publish a new binary without a separate green main release job.

The synthetic scheduling interleaving establishes correctness of the checked state transition, not a measured percentage of avoided real-user disruptions. Physical Windows 11 at 200% display scaling, actual disconnected network roots, time-to-first-use measurements and long multiday project changes still require separate acceptance.
