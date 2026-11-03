# Manual layout anchors must appear in slot-conflict feedback — 2026-11-03

## Practical scenario and evidence

Continue [manual mode stability](2026-10-27-manual-mode-stability.md) and [explicit offline priority](2026-11-02-explicit-always-offline-auto-capacity.md); no broad literature search is repeated.

A person manually positions a frequently used shortcut while leaving its app rule set to **Auto**. Later, they mark a different, rarely used program as **Always** and reduce the desktop selection to one slot. The saved manual position and the new Always rule both legitimately need a slot. The production selector refuses to silently discard either explicit choice, and `engine::preview_loaded` correctly refuses a two-item selection when only one slot exists. However, the Apps page previously counted only `app.valid && policy == Always`, displaying **no visible conflict warning** in this case (one Always rule, one available slot). The user instead saw an error only when trying to apply.

This is a source-confirmed inconsistency in Desktop Pilot's feedback, **not** evidence of how frequently people encounter it. The [CHI 2004 comparison of static, adaptive and adaptable menus](https://doi.org/10.1145/985692.985704) establishes the importance of user control and accurate feedback in adaptable interfaces; it does not measure this application's usability. In a [November 6–18, 2025 Fences support thread](https://forums.stardock.com/539431/fences-6-cant-restore), some users reported that restored icon arrangements did not match their manual preferences. These are observations about another tool, not a Desktop Pilot failure rate.

## Fix

The Apps page now shows the number of **distinct manually reserved slots**, not just valid Always rules. The count includes:

- A valid app with an explicit Always rule, whether or not it has been placed yet.
- A previously manually positioned, owned shortcut whose original is still valid, even while its rule remains Auto.
- An owned Always or manually positioned shortcut with a known unchanged-but-offline source, preserving the existing offline retention behavior.

It does not count duplicate ownership rows twice, missing app identities, revoked **Never** choices, or an offline entry with no valid source and no preserved outage identity. The manual placement and Always rule are counted together if they refer to the same app. The warning explains how to increase capacity or release a manual position/rule; the precise Apply guard remains unchanged.

The implementation adds no preference or database field, no automatic rearrangement, no new background work, and no telemetry.

## Reproduction and checks

`model::manual_slot_conflict_tests::manual_auto_anchor_and_new_always_rule_report_a_conflict_before_apply` uses actual native-parsed `.url` shortcuts, an isolated SQLite `Store`, a real owned desktop copy and the production `engine::preview`:

1. **Before:** one manual Auto shortcut plus one new Always shortcut in one slot -> Apps warning counted **1** and was absent, while preview rejected the conflict.
2. **After:** the same stored state requires **2** slots, and the same production preview still rejects it without touching originals or the existing desktop copy.
3. Choosing **Never** for the original manual anchor drops the requirement to **1** and allows the intended preview. Reverting the rule restores the two-slot explanation, while all original files survive.

`offline_manual_reservations_count_once_and_never_rules_release_them` covers duplicated legacy ownership, six-day offline retention, missing source identities and explicit revocation.

Run existing Windows Server 2022/2025 strict checks unchanged: `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, native integration fixtures, and release guards. These scripted sequences are synthetic application-state regressions, **not** evidence of real-world minutes saved. The actual Settings window at 100–200% scaling, keyboard/screen-reader phrasing, physical Windows 11 workflow and the independent organizer drafts #98/#101 still require separate acceptance. No release should be described as verified until its checks complete.
