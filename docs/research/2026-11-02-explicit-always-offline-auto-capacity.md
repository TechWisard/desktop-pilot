# Explicit Always while an Auto shortcut is offline — 2026-11-02

## Scope, prior evidence and hypothesis

Continue [transient unavailability](2026-10-26-transient-unavailability.md), [capacity and recovery](2026-10-26-capacity-and-usage-recovery.md), [manual outage stability](2026-10-30-manual-long-outage-stability.md), and [compound manual recovery](2026-10-30-compound-manual-recovery.md). Do not repeat the general literature survey.

Findlater and McGrenere's [CHI 2004 controlled comparison](https://doi.org/10.1145/985692.985704) motivates predictable and user-controlled adaptation. [Stardock Fences user reports from November 2024](https://forums.stardock.com/532016/moving-fences-moves-single-desktop-icons-as-well-causing-chaos) describe unwanted movement of unrelated icons in another application. These are **external observations**, not evidence of a Desktop Pilot failure rate, correctness, or time saved.

**Confirmed source-level defect at `6873e357`:** in a full two-slot Desktop Pilot layout, a source can disappear for under 72 hours. Its existing Auto quick copy is temporarily retained and treated as fixed. When the user explicitly chooses **Always** for a new valid app, the selector temporarily holds three candidates. The over-capacity removal loop excludes the offline Auto copy because of its soft grace, so it evicts the working Auto copy instead. This conflicts with the user's new explicit choice and the program's existing capacity-reduction preference for usable shortcuts.

## Before / after production-path regression

Synthetic setup: two owned Auto entries, an offline source (unavailable for one hour) with high historical score, a still-working regular shortcut with lower score, and one new valid user-selected Always shortcut. Capacity remains **2** throughout; no unusual config change is needed.

- **Before:** offline Auto + new Always are selected; working Auto is unnecessarily removed. One usable shortcut is displaced, and the temporarily inaccessible shortcut occupies the remaining regular slot.
- **After:** working Auto + new Always are selected; offline Auto is the displaced candidate. There are still only two slots; the explicitly selected app appears without sacrificing the usable existing Auto shortcut.
- **No new choice:** offline Auto + working Auto remain stable under the usual grace period.
- **Offline manual placement:** the explicitly manually positioned offline shortcut remains protected. Such hard choices are not silently overridden.
- **Manual-only mode:** deliberate Always is honored without allowing score-driven automatic placements.
- **Undoing the policy selection:** returning the new shortcut to Auto restores the original preview while the outage grace still holds.

`model::offline_explicit_choice_tests` includes both direct production selector scenarios and an actual Windows native `.url` catalog stored in SQLite, queried through `engine::preview`. Original online and newly selected shortcut bytes, persisted user policies, and preview add/remove counts are checked. This is a **synthetic correctness regression**, not a measured real-person improvement. It deliberately does not stage or rearrange a real Explorer desktop during headless CI.

## Minimal change and remaining acceptance

Distinguish **hard fixed** entries (Always or manually positioned) from **soft fixed** entries (Auto kept by offline grace). For the *over-capacity removal pass only*, soft grace is not a prohibition on removal; prefer removal of an unavailable Auto before a working Auto. For normal score-based challenger replacement when the layout is full, keep the original soft-grace protection and minimum advantage and change-budget behavior.

Run unchanged Windows Server 2022/2025 tests, formatting, strict Clippy, build, native integration and publication guards on the candidate and the eventual main SHA. Preserve current profile data, shortcut bytes, rollback semantics, unchanged permissions, offline operation, and lack of telemetry. Physical Windows 11 tests of removable disks and multi-monitor icon positions, plus longer project-switch sequences, remain outstanding.
