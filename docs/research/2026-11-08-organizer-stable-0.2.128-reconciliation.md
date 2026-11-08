# Stable 0.2.128 and unreleased organizer reconciliation — 2026-11-08

## What was reconciled

The shipped stable [v0.2.128](https://github.com/TechWisard/desktop-pilot/releases/tag/v0.2.128) includes a fix to packaged-app identity attribution. When two saved launch profiles share a Windows AppUserModelID and one launch source temporarily goes offline, foreground evidence must not be silently credited to the remaining profile. The native Windows/SQLite regression in `src/engine/profile_attribution_tests.rs` covers this. Its production matcher change is applied to the exact previous implementation in the organizer staging branch; the organizer-specific engine changes remain intact.

The stable release documentation, source regression, and bilingual release explanation are carried into staging. Staging keeps version 0.3.0 in `Cargo.toml` and `Cargo.lock` because the organizer is **unreleased**. Do not replace these with stable 0.2.128, release a 0.3.0 ZIP, or overwrite the stable main branch. The merge has both source-history parents, retaining all past changes. The existing 45-shortcut organizer exclusion-reason search regression (#236) is also preserved.

## Evidence and limitations

Confirmed source mismatch: the previous `observe_app_id` filtered invalid aliases before checking launch identity uniqueness. The stable corrected code determines ambiguity across all saved matching identities, then checks the sole candidate's availability. This is separate from customer reports about browser-profile launch confusion (e.g. [PowerToys issue #38900](https://github.com/microsoft/PowerToys/issues/38900)), which are not evidence of this particular defect. Research on predictable adaptation is recorded in [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md), and the stable first-hand regression in [packaged-app attribution](2026-11-08-packaged-app-alias-attribution.md).

Expected synthetic pre-/post-condition: one available and one offline alias with the same app identity previously credited the valid alias; it now attributes neither; a genuinely unique available alias still matches. Saved user rules, positions, and bytes remain unchanged. This is a code/test correctness claim, **not** measured user productivity.

## Required acceptance

After staging this merge, confirm the proposed exact-head Windows 2022/2025/11 ARM checks (locked Rust tests, strict Clippy, build, native Explorer, UI and redirected Desktop fixtures). Compare the merge tree and its two parents; recheck README release claims and `Cargo.lock` consistency. Outstanding release gates remain physical Windows 11 with display changes, actual OneDrive redirection, interactive keyboard use, and interruption/recovery. Staging stays draft until then.
