# Manual mode and newly detected taskbar pins (2026-10-30)

## Starting point and evidence

The inspected baseline was public main fa6bbeff9074141c37df025544af378aaca8b3f5 (0.2.65 release commit, checks still in progress at inspection), with no open PRs. No change to repository visibility or ordinary Windows CI is needed. This follows the documented Manual-mode contract in [manual-mode-stability.md](2026-10-27-manual-mode-stability.md), rather than repeating that research.

Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) compared adaptive and user-customized layouts and motivated predictability and direct control. A PowerToys Command Palette Dock report, [#46564](https://github.com/microsoft/powertoys/issues/46564) (2026-03-27), describes pinned shortcuts unexpectedly disappearing in a *different* product. Both are independent motivation, **not** observations of Desktop Pilot users or measured time savings.

## Source-confirmed inconsistent sequence

The production model::select_at deliberately stops before score-driven placement in Manual mode (automatic=false, collect=false). But an earlier filter of already owned Auto shortcuts required them not to be taskbar-pinned. Consequently, an existing quick copy could be scheduled for deletion on ordinary Manual Apply after the user added a Windows taskbar pin, without any Always/Never action or capacity reduction. Taskbar filtering should still exclude *new suggestions*, and Observe/Adaptive may remove a redundant existing Auto copy. Manual mode promises that an existing copy does not move until an explicit user action.

A failure-first branch, fix/manual-mode-retain-pinned-copy-20261030, adds selector and real SQLite/.url/engine::preview regressions, including explicit Never, Observe, unchanged original bytes and saved ownership. CI results must be inspected rather than assumed.

## Narrow correction and acceptance

Only the retention predicate for **already managed, valid Auto** copies recognizes Manual mode before applying the taskbar-pin filter. Candidate eligibility, taskbar-pin detection, Always/Never, capacity, disabled collection and Undo are unchanged. The Manual-mode return still prevents all score-driven addition and displacement. No new setting, telemetry, service, database migration or UI control.

Tests use production ranking/selection and real SQLite preview, not a physical Windows 11 taskbar. Run unchanged formatting, locked Rust tests, strict Clippy, locked build, native scenarios and publication guards on the two public Windows runners. Green synthetic fixtures do not prove human time savings. Physical taskbar transitions, DPI 200%, drive outages and longer user-workflow measurements remain in issues #22 and #56.

## UI caveat

The recommendation list can still state that a taskbar-pinned candidate is ineligible for *new* placement while a previously managed copy remains in Manual mode. These are different rules; a physical UI comprehensibility check is still required.
