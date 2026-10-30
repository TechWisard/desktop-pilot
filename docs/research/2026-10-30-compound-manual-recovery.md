# Combined manual control, offline originals and reversal (2026-10-30)

## Baseline and scope

Candidate baseline: `main` `2f52673311562b52886ac76625b982d6e3b53986`, Desktop Pilot 0.2.68 source. Main Windows workflow [41094336392](https://github.com/TechWisard/desktop-pilot/actions/runs/41094336392) completed successfully on the unchanged baseline. The separately published latest release must be confirmed from GitHub before making any release claim. No open PR existed when this branch was opened. The public `desktop-pilot` repository already runs normal Windows CI; the unrelated `finite-checks` stand is neither necessary nor appropriate for a whole-product integration scenario.

This stage combines previously **separately** covered manual pin retention, long source absence, explicit Always/Never, operating-mode changes and transactional Undo. It is an acceptance/regression extension, **not** a newly confirmed user-facing production defect and not a reason by itself to publish another binary.

Primary HCI sources already indexed in [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md): Findlater and McGrenere, CHI 2004, https://doi.org/10.1145/985692.985704 (27 participants: direct control matters; not a Desktop Pilot measurement); Findlater et al., CHI 2009, https://www.cs.ubc.ca/labs/imager/tr/2009/findlater_chi_ephemeral/ (spatially stable adaptive presentation). An independent [PowerToys Workspaces discussion](https://github.com/microsoft/PowerToys/issues/34597), opened September 4, 2024 and discussing taskbar/workspace shortcut preferences through 2026, illustrates how people mix quick access routes. It is not a report of a Desktop Pilot fault.

## Executable sequence and invariants

`src/model/compound_manual_control_tests.rs` uses **real** `Store`/SQLite, `native::read_link` for three inert `.url` files, `rank`, `engine::preview_loaded`, `engine::preview`, the production `engine::apply` transaction, `engine::command("rollback")` and `Store::policy`/mode persistence. No separate selector, mocked database or external application is launched.

Synthetic sequence:
1. Two already managed quick copies occupy a Manual profile. One app becomes pinned on the taskbar; the other unchanged source is missing for ten days, but its owned copy still exists. A third, accessible app is explicitly Always.
2. Before Apply, expect the existing pinned copy and the explicit Always candidate, **not** the long-offline Auto copy, when capacity is two. Compare the persisted Store preview with direct production selection.
3. Apply on an isolated desktop. Verify the pinned copy stays byte-identical, the offline owned copy is removed, the selected new copy matches its original, and no original file is changed.
4. Undo the actual filesystem transaction. Verify both saved copies are restored, the newly created copy is gone, and the still-offline original is untouched.
5. Set explicit Never on the original pinned app. Verify the preview excludes it without moving files until Apply. Switch to Observe and verify the expired offline Auto copy is no longer promised retention. Check that policy, source bytes, and existing copies remain intact.

Metrics available from this deterministic fixture: selected/added/removed identifiers, actual quick-copy file counts and bytes, rule persistence and Undo state. **No user task duration, real taskbar reading, cloud outage, actual taskbar pin interaction, Windows 11 DPI behavior or measured human time saving is claimed.**

## CI and acceptance

Run the unchanged `cargo fmt --check`, `cargo test --locked`, strict `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `scripts/test-native.ps1`, `scripts/test-release-workflow.ps1` and README tests on the existing Windows 2022 and 2025 runners. Do not weaken release gates to make a fixture green. If an assertion is invalid, correct the fixture openly; if production behavior is defective, record a failure-first run before a minimal code correction. Retain tests without creating a new product release if the baseline behavior is already correct.

Next high-value acceptance remains issue #22: actual Windows 11/200% interactive use, long real network/source outages, repeated restarts with multi-thousand-entry custom catalogs, and observational evaluation of unnecessary changes over changing projects.
