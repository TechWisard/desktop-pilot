# Manually protected shortcuts after extended source absence — 2026-10-26

## Evidence versus hypothesis

Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004, 27 participants), found that user-controlled arrangements were generally preferred and could outperform shifting automatic ones: https://doi.org/10.1145/985692.985704. Findlater et al. (CHI 2009) separately demonstrated that visual-search assistance can preserve spatial consistency: https://www.cs.ubc.ca/labs/imager/tr/2009/findlater_chi_ephemeral/. Microsoft documents missing Windows desktop icons as a user-facing recovery problem with multiple possible causes: https://support.microsoft.com/en-us/onedrive/where-have-my-desktop-icons-gone. None of these sources measures Desktop Pilot or establishes the optimal outage duration.

**Design hypothesis:** A shortcut the user chose **Always** or physically moved should remain in its existing space even if its unchanged source stays unavailable longer than the ordinary 72-hour grace period. Automatic shortcuts can expire normally; an intentional Never or Auto rule, a changed target identity and a deleted/edited managed copy must remain authoritative.

## Confirmed 0.2.9 defect and exact reproduction

In `model::select`, manual priority was checked only **after** initial filtering of invalid apps. An unavailable shortcut fell out of selection once its 72-hour grace expired, even if `Owned.manual` or `Policy::Always` protected it. On `engine::apply`, the existing verified quick copy could be removed and replaced by a frequently used app; after the source returned the Always copy could be reintroduced, causing two avoidable changes.

Test-only baseline: https://github.com/TechWisard/desktop-pilot/actions/runs/40335071749. Formatting passed. On Windows 2022, **170 existing tests passed and two new production-logic regressions failed**: `model::tests::manual_anchor_survives_a_six_day_outage_but_explicit_release_and_changes_do_not` and `engine::tests::manually_pinned_rare_shortcut_survives_long_absence_then_obeys_release`. This reproduced incorrect selection with real Rust logic, SQLite profile, on-disk quick copy and simulated six-day absence, not a physical drive disconnect.

## Correction and safety boundaries

Only an **already-owned** quick copy that remains byte-identical and is marked unavailable (`unavailable_since > 0`) receives the manual retention exception. No new shortcut is created from a missing original. The default 72-hour limit for automatically selected shortcuts is unchanged. An explicit Never, changed original launch identity, verified user removal/edit and an explicit policy change are still honored. The existing catalog-warning mechanism explains the protected offline state without adding normal-operation controls. A capacity conflict with multiple manual anchors is still reported rather than overriding manual choices.

The integrated scenario uses actual `Store`, `refresh_apps`, `preview`, `apply`, `reconcile_manual`, shortcut copying and `command(policy:Auto)`: initial Always placement, source removal, expiry after six days, no unsolicited replacement, source restoration with identical path/bytes/position, then explicit Auto release and one justified replacement. The selection-level test additionally covers legacy manual ownership, Never and changed-identity overrides.

**Synthetic before/after:** At six days of unavailability, the old selector proposed one unrequested replacement and the corrected one proposes zero. This is not measured human efficiency.

## Regression reconciliation

The first corrected implementation exposed two older tests that implicitly used `add_test_app`'s `Always` default while expecting Auto behavior. The source-reconnect test now expects the stronger explanation for a deliberately protected copy. The separate 72-hour-expiry test explicitly creates an **Auto** shortcut with genuine historical eligibility before placement, so it continues to verify that ordinary automatic copies really disappear after the grace window. The test asserting rejection of changed launch identity remains unchanged. These are substantive scenario disambiguations, not skipped or weakened assertions.

## Remaining verification

Run formatting, Rust tests, Clippy with warnings denied, build, native Windows fixtures and publication guards on both supported CI runners. Test actual unplugged devices, Windows 11 Explorer icon positions, Russian/English warning readability at 200% scaling and varied long-term project workflows separately. No changes to private user files, telemetry, schema, privileges or network activity.
