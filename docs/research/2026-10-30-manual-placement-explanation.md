# Explain retained shortcuts separately from new-placement eligibility (2026-10-30)

## Baseline and evidence

Baseline: public main `d67ac67643cacbadb2b1ccd11bc94b247fbbb317` (Desktop Pilot 0.2.67 source). No open PRs when the work began. Main Windows checks succeeded on Server 2022/2025, but publication of 0.2.67 failed at `gh release create` with upstream GitHub HTTP 500, so the release must not be claimed published until a successful retry and asset verification.

Existing primary HCI evidence: Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus*, CHI 2004, https://doi.org/10.1145/985692.985704 (27 participants, a controlled comparison, not Desktop Pilot). The study motivates predictable, user-controlled placement, but cannot establish Desktop Pilot usability outcomes. Independent PowerToys Command Palette Dock issue #46564, https://github.com/microsoft/PowerToys/issues/46564 (reported 2026-03-27), describes missing pinned shortcuts **in another product**. This is contextual evidence only, not a report of a Desktop Pilot defect.

The Desktop Pilot-specific inconsistency comes from its actual source: `model::rank` correctly describes a taskbar-pinned Auto item as ineligible *for a new shortcut*, and an offline item as unavailable. Meanwhile `model::select_at` deliberately retains the **already owned** item in Manual mode even after a taskbar pin or prolonged unchanged source outage (regressions documented in `2026-10-30-manual-taskbar-pin-stability.md` and `2026-10-30-manual-long-outage-stability.md`). The Details dialog displayed the rank reason with no distinction. An existing shortcut could therefore appear excluded in Details even though Apply would keep it. The underlying placement logic is correct; this is a **confirmed explanation mismatch**, not evidence of unexpected filesystem mutation.

## Reproduction and acceptance

A production-`Settings` regression in `src/ui/manual_placement_reason_tests.rs` creates genuine isolated `.url` sources, persists app records, policies and ownership in actual SQLite `Store`, renders the genuine egui Details dialog, and compares the visible text with the real `rank` output.

It checks these sequences, not only individual labels:
- **Manual + already owned + newly pinned**: preserve the desktop copy and explain that the taskbar pin alone does not remove it.
- **Manual + already owned + unchanged ten-day outage**: explain that the copy stays put while launching may fail; never promise that an unavailable source can launch.
- **Switch to Observe**: stop promising Manual-only retention.
- **Explicit Never while returning to Manual**: do not imply preservation after an explicit exclusion.
- Verify the original shortcut bytes stay unchanged.

Correction: a narrow, state-aware explanation only in Details, guarded by the saved operating mode, existing ownership, validity, the visible effective manual policy, and taskbar setting. It does not alter `rank`, `select_at`, preview, SQLite, worker lifecycle, placement, Undo, or actual filesystem state. No new widget, preference, telemetry, external service or dependency.

## Verification boundary

Record failure-first Windows checks for the test-only branch, then green Windows 2022/2025 checks for the corrected branch. Do not mistake a compilation/formatting error for proof of the behavioral defect. Require unchanged Rust tests, strict Clippy, build, native integration fixtures, release-guard fixtures and README checks. Do not merge without complete green checks. If a version is warranted, publish separately from verified main and check the source SHA, tag, build and assets. Neither render fixtures nor Windows Server CI prove real Windows 11/200% DPI usability or human time savings; issue #22 remains open for physical and longitudinal acceptance.
