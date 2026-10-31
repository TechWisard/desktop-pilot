# Duplicate ownership rows must not revoke offline grace — 2026-11-01

## Confirmed source-level defect

Desktop Pilot already deduplicated old `State.owned` records by application identity when constructing quick-slot selections, but the separate `capacity_reduced` decision still compared the **raw row count** against the configured slot capacity. An old profile with two rows for one copy and one row for another could therefore appear to have three occupied slots when the user actually had two unique shortcuts. This falsely disabled the existing 72-hour temporary-unavailability protection. A newly popular project could then replace the temporarily disconnected shortcut at the next review, even when no slot reduction was requested.

This is a deterministic source-code inconsistency, not a verified field report or a measurement of real user productivity. Existing background research in [adaptive shortcuts](2026-10-26-adaptive-shortcuts.md) documents spatial stability and manual-control findings from Findlater and McGrenere (CHI 2004) and predictability research from Gajos et al. (CHI 2008). The distinction between a temporary network/source outage and an explicit capacity reduction is a user-behavior hypothesis selected for regression coverage, not a population claim.

## Reproduction and acceptance on the application path

`model::cold_start_tests::duplicated_legacy_ownership_does_not_evict_an_offline_shortcut_during_grace` writes actual inert Internet Shortcuts to an isolated profile, creates the two managed desktop files, removes one original to model a disconnected source, and persists legacy duplicate ownership rows in the actual SQLite Store. The original has been offline for one hour. A new project has a strong saved priority and the other incumbent has an explicit Always rule. The test calls `engine::preview`, which uses the real Store, ranking and `select_at` implementation.

With **two available slots and two unique owned apps**, the expected proposal is unchanged for three repeated previews: zero additions, zero removals, the offline shortcut and manually selected app still selected. Before correction, the raw count of three ownership rows incorrectly disabled outage protection, allowing the high-priority newcomer to replace the offline incumbent. The correction counts **distinct application identities** for this capacity decision only; it leaves the existing duplicate-filtering, candidate scoring, permissions and manual-rule handling unchanged.

The same sequence checks that **reducing the configured capacity to one** does remove the offline Auto app while retaining the explicit Always app; returning to two slots and changing the offline app to Never prevents it from reappearing. The test verifies that neither managed shortcut's bytes change during previews and the original source is still absent. No new background work, telemetry, schema, migration or external service is introduced.

## Evidence and remaining gates

The baseline test-only branch is `fix/duplicate-owned-offline-grace-20261101`; the fixed branch is `fix/duplicate-owned-offline-grace-complete-20261101`. Compare results only when the exact matching Windows CI runs are complete; formatting failures are not counted as behavioral reproduction.

Require standard public Windows Server 2022 and 2025 formatting, locked Rust suite, strict Clippy, build, native integration fixtures, release guards and a merge-head check before distribution. An isolated Windows test does **not** establish the frequency of legacy duplicate rows, interactive UI correctness at Windows 11 200% scaling, network drive recovery timing, or human time savings. No user files/settings/Undo are modified by the correction.
