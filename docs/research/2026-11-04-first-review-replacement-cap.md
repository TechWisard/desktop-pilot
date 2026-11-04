# First review must respect the replacement cap — 2026-11-04

## Observed source behavior and scope

Version 0.2.109 introduced zero automatic replacements so a vacant desktop slot can be filled without displacing a familiar shortcut. The selector previously skipped every replacement-budget check when its initial argument was true, even when State.owned already contained real desktop copies and State.initialized was false. apply_locked enters initial selection when the saved initialized flag is false. A restored or interrupted setup can therefore enter that path with an occupied desktop.

This is a **source-confirmed boundary defect**. The regression uses a simulated interrupted/restored state; its incidence and real-user time cost have not been measured. It differs from truly fresh onboarding with no owned copies.

## Previously collected evidence (not a new user study)

- [Predictability and Accuracy in Adaptive User Interfaces, CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml): predicts importance of spatial consistency, but does not test Desktop Pilot.
- [Fences report on surprising icon moves, 2024-11-06](https://forums.stardock.com/532016/moving-fences-moves-single-desktop-icons-as-well-causing-chaos): anecdotal report about another product, not a Desktop Pilot failure measurement.
- [Earlier zero-replacement scenarios](2026-11-04-zero-replacements.md): explicit zero budget, vacancies, manual exceptions and file stability.
- [Interrupted placement and undo](2026-10-29-project-change-undo-acceptance.md): reversible changes and familiar layout.

## Correction and acceptance

Apply the existing max_changes budget to every ranked replacement, including first reviews with pre-existing owned entries. No new settings, data format, collection, UI elements, services or destructive actions. An empty first-time desktop can still fill its eligible vacant slots without spending a replacement.

Check the real model::rank and select code on synthetic data:

1. Two managed Auto shortcuts and two stronger newcomers; initialized=false, max_changes=0: no displacement.
2. One managed Auto shortcut with one vacant place: one newcomer fills the vacancy.
3. max_changes=1: at most one replacement even on initial selection.
4. Preserve existing Always/Never, shortage, offline, manual-mode and default-one tests.

Also exercise the real Windows engine::preview and engine::apply paths with SQLite Store and local .url files: create two owned copies, add a higher-scored newcomer, simulate an unfinished initial-review checkpoint, preview and apply. Require no addition/removal; original bytes and managed file identity and position must remain intact; subsequent explicit Never releases a place for the newcomer.

**Synthetic correctness metrics:** displacements at zero budget = 0; useful vacant-slot fills in one review = 1; default-one cap displacements at most 1; source bytes and managed positions unchanged during no-op. These are not measured human productivity improvements.

## Verification boundaries

Require the normal Windows Server 2022/2025 pipeline (locked Rust tests, formatting, strict Clippy, build, native integration and publication guards). Passing those checks cannot establish Windows 11 live UI scaling or physical long-duration interruption behavior. Work-in-progress organizer PRs #98 and #101 remain separate.
