# Explain zero-move plans with checked singleton groups — 2026-11-08

## Confirmed logic and practical ambiguity

The production `Plan::movements` intentionally avoids creating a new destination folder for just one checked item. In the organizer review, however, that item's checkbox stays checked even though the summary says **0 moves**. A person who deselects one of two site shortcuts can then see the remaining shortcut checked beside **Remember keep choices**, with no explanation of why the checked shortcut cannot move. The same occurs on a fresh desktop with a single eligible shortcut.

This is a verified source/UI-state inconsistency, not a claim that anybody has lost work or that the product saves measured time. Existing policy is safe and should remain: no singleton new folder, no automatic moves, no fake Undo entry. Research on adaptable interfaces emphasizes understandable and predictable changes: Findlater and McGrenere (CHI 2004), https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/ ; Gajos et al. (CHI 2008), https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml . The link from those findings to the exact wording here is a design hypothesis.

## Narrow correction

When **at least one item is checked** but the actual production plan reports **zero moves**, explain that a new folder requires two checked items from the same group. Display **Remember keep choices** only when at least one unchecked item can actually be remembered, and **Apply selected moves** only when at least one movement exists. Suppress the previously visible, disabled no-op confirmation for an all-checked singleton review. Do not change any selection automatically. Keep the normal global and filtered selection controls, one-operation recovery, protected items, manual rules, and remembered exclusions untouched. No new modal or settings.

## Reproducible combined sequence

The Rust `zero_move_review_explains_singletons_without_moving_or_forgetting_user_choices` regression exercises the real `Settings::organization_dialog`, `organize::analyze`, `Plan::summary`, asynchronous `remember_keep`, Windows Internet Shortcut parser, SQLite persistence and read-only UI paint. Two real temporary `.url` files start in the same group. Deselecting one changes the plan from two moves to zero, but leaves the other checked; the explanatory text and truthful **Remember** action must appear. After Remember and a fresh analysis, the remaining singleton is still checked and still cannot move, and no action is falsely offered. Both original files retain identical bytes, exactly one keep policy is persisted, and no Undo journal is created.

**Expected before/after on this synthetic fixture:** unqualified checked zero-move states with no explanation **2 → 0**; actual moves **0 → 0**; additional confirmation dialogs **0 → 0**. This is a correctness/clarity check, not real-world human time savings.

## Release boundary

The existing public Windows CI must pass formatting, locked Rust tests, strict Clippy, build and applicable native, Explorer and bilingual UI checks on the exact proposed head. Real physical Windows 11 keyboard, multi-monitor/DPI, active OneDrive and interrupted-power acceptance remain open. This correction belongs only in the unreleased organizer draft until those checks are completed.
