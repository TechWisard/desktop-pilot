# Unavailable shortcut preview after a backward clock correction (2026-10-26)

## Scope and evidence

This continues the [clock-correction investigation](2026-10-26-clock-correction-recovery.md) and the [manual-absence work](2026-10-26-manual-absence-completion.md) from the published 0.2.11 baseline. It is not a new ranking policy.

**Primary evidence:** Microsoft documents that the Windows system clock can change in either direction and is not suitable for measuring elapsed time: https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time . A user discussion about OneDrive's Desktop backup changing shortcut availability is relevant operational context, not proof that Desktop Pilot has experienced this particular failure: https://www.reddit.com/r/onedrive/comments/1niugow/onedrive_removed_all_my_desktop_icons/ (2025-09-16). The existing research on predictable adaptive layout and manual control is cited in [adaptive-shortcuts.md](2026-10-26-adaptive-shortcuts.md).

**Confirmed from current source:** `refresh_apps` repairs a saved future `unavailable_since` on its next filesystem scan. However, `Settings::reload` and `engine::preview` read already persisted state **before** forcing another scan. `model::select` previously required `unavailable_since <= now()` even for an existing **Always** or manually moved copy. If the system clock was corrected backward while the original shortcut was absent, the preview presented a protected shortcut as a removal, and a more popular application as its replacement. The actual `apply` performs a rescan and was already safer than its own preview. Contradictory preview/action behavior undermines an informed user decision.

## Correction and bounded behavior

For an **already managed** copy, a positive outage timestamp in the future means its elapsed absence is unknown, not that the user withdrew permission to keep it. `select` now retains a copy if it is manually protected, within normal 72-hour grace, **or** its saved absence timestamp is in the future. The actual scan resets a future timestamp before an apply, so standard Auto grace and expiry remain in force thereafter.

Unchanged safety conditions:
- A new offline candidate cannot receive a place; the original must be readable and match the stored launch identity before copying.
- A changed source identity has `unavailable_since = 0` and receives no protection.
- The user's **Never** rule wins, including after a clock correction.
- Explicit capacity reduction still prefers a usable copy when there is no manual anchor; no unrelated preferences, originals, file identities or desktop positions are rewritten.
- No new persistence field, network activity, telemetry or UI control is introduced.

## Failure-first regression and compound fixture

1. `model::tests::corrected_clock_must_not_make_a_known_offline_copy_look_unprotected` replays a persisted checkpoint 14 days **ahead** of current time. A rare existing Always copy must beat a high-scoring replacement; an older manually moved Auto copy and an ordinary Auto copy in the now-uncertain grace period also remain. A never-managed offline candidate remains ineligible, as do changed identity and explicit Never cases.
2. `engine::tests::offline_clock_rollback_preview_matches_safe_apply_and_manual_release` uses the actual `Store`, Windows .url parser, filesystem, `refresh_apps`, `preview`, `apply`, and `command`. It places one deliberately chosen shortcut, removes its source, persists the future checkpoint, opens the preview, applies, checks the exact shortcut bytes/path/file identity/position, then selects Never and replaces it with an available alternative. An arbitrary high competitor score cannot displace manual choice.

Expected prior-state preview: **one proposed removal and one replacement** in the synthetic 1-slot fixture; expected corrected preview: **zero proposed changes**. This is a deterministic regression outcome, not measured productivity or user timing.

## Verification record and handover

The branch was based on the immutable 0.2.11 source commit `5e928d40b9551b0f731ba26f72c9d15a7c54a8e4`. Test-only commits `7abb736d60e976d92607fb6650970c1a1a08d9f1` and `09457ce9290cfab3fd7e46f233d6b4b6f6575b6a` preceded implementation commit `5f2ed87fae3ce8885b4e33b9aa6fed784aec0b1b`. Check the final branch's full Windows 2022 and 2025 CI results, not a cancelled intermediate workflow, before merging. The unchanged CI performs formatting, Rust tests, strict Clippy, debug build, native integration fixtures and release guards.

Unverified: a physical Windows 11 clock correction during an actual OneDrive outage, Explorer icon behavior on dock/undock, and human task-time impact. Follow up separately on genuine removable-drive disappearance of a custom profile, simultaneous manual edits during scans, and the first useful suggestion after a project change.
