# Organizer fixed header: accessible exit and single confirmation (2026-11-07)

## Observed failure, not a user-time claim

On the actual draft organizer UI, the title, Apply/Remember and Close
shared a wrapping horizontal row. In hosted Windows screenshots at 200%
scale, Close moved onto a second row, while the confirmation occupied
the first. The existing research and screenshot review are recorded in
[organizer-scroll-action](2026-11-07-organizer-scroll-action.md) and
[PR #211](https://github.com/TechWisard/desktop-pilot/pull/211).
This is reproducible widget placement, not a measured human productivity result.
The adaptive-interface study by
[Gajos et al. (CHI 2008)](https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/)
provides background on predictability, not proof for this particular change.

## Fix and production-path regression

Keep title and Close on one fixed row; align Close opposite the title.
Place the only Apply/Remember action on its own fixed row, never inside
the scrolling plan. This prevents a wide action label from moving the exit
control at higher zoom. The existing permission gating and action handlers
remain unchanged.

`src/ui/organization/tests.rs` now renders the real Settings modal and a
real analyzed two-shortcut plan in English/Russian at 100% and 200%.
It checks that the title and Close have overlapping vertical extents
(and separate horizontal extents), that exactly one Apply is painted on
a fixed second row, then actually clicks Close and verifies no file moves
or jobs were started. Existing 45-item scroll, recovery, busy-state,
apply/undo and long-preview UI tests continue to exercise the same code.

## Verification boundary

Run the unchanged Windows fmt, cargo test, strict Clippy, build, Explorer,
cloud-root and native UI checks on the exact commit. Hosted geometry and
single-display screenshots do not replace physical Windows 11,
multi-monitor/DPI transitions or manual keyboard-focus accessibility review.
Do not release the organizer until its separate acceptance gates pass.
