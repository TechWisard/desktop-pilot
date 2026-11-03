# Shared process roots must not identify an unavailable launch profile — 2026-11-03

## User scenario, sources, and limits

Continue the existing [process/profile attribution research](2026-10-28-profile-process-attribution.md) and [adaptive-interface predictability study](2026-10-26-adaptive-shortcuts.md), rather than repeating discovery. The Windows [GetWindowThreadProcessId documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid) confirms that a foreground-window lookup identifies a process, not the shortcut arguments that started it. A [PowerToys Workspaces report about Chrome profiles](https://github.com/microsoft/PowerToys/issues/35187) illustrates the general risk of conflating launch profiles. It is **not** evidence of a Desktop Pilot user incident.

A person saves two launch profiles using the same installation root; their shortcuts have different arguments. The second shortcut becomes unavailable (disconnected volume, removed shortcut, temporarily invalid target) while a process from that root remains running. The program must not credit all observed usage to the first profile merely because it is the only *currently valid catalog record*. An explicit app identity, if actually verified, can still resolve a launch elsewhere in the foreground-matching pipeline.

## Confirmed production-path inconsistency

In `engine::observe_match`, the direct-executable branch already treats an unavailable argument-bearing shortcut as a possible competing profile. However, the *observed-root* branch discarded `!app.valid` entries **before** collecting matching roots. With two distinct identities sharing an observed root, one temporarily unavailable, the existing root matcher returned the remaining profile's ID for a path under the shared root. This could create a false SQLite session, raise a recommendation score and eventually select a quick shortcut for the wrong profile.

The repair computes root-identity ambiguity across **all** saved matching catalog records, including temporarily unavailable ones. If more than one distinct identity matches the process root, no process-only attribution is made. The ordinary unique-root attribution is unchanged; an explicit Never/Always rule does not become automatic evidence.

## Regression scenarios and measurable outcomes

The `engine::profile_attribution_tests` fixtures use real Windows `.lnk` creation/parsing, a local SQLite `Store`, the production `observe_match`, usage writer and selection preview:

1. **Shared root, competing shortcut offline:** the root matcher must return `None`; SQLite must contain **0** attributed daily rows and preview **0** placements. The earlier matcher returns the remaining Work profile instead. Restoring the second shortcut must still leave the root ambiguous.
2. **Distinct roots:** Work and Private have separate roots; a process under Work must resolve Work even if Private is temporarily unavailable. Its confirmed usage generates **1** correctly attributed SQLite row and **1** eligible preview item.
3. Existing direct executable, parameterized shortcut, unavailable direct profile, normal installed-app, manual-choice and rollback regressions remain authoritative.

This is a synthetic reproduction using actual application components, not observation of live game launches, proof of profile-specific process telemetry, or a measurement of minutes saved. More conservative attribution may yield fewer automatic suggestions for genuinely ambiguous apps; explicit manual Always placement remains available.

## Verification and next work

Run unchanged public Windows Server 2022/2025 CI: formatting, locked Rust tests, strict Clippy, build, native integration and release-publication guards. The [failure-first test-only branch](https://github.com/TechWisard/desktop-pilot/tree/fix/offline-shared-root-attribution-20261103) isolates the regression from the fix; [its initial run](https://github.com/TechWisard/desktop-pilot/actions/runs/41604517532) stopped at formatting only, so it is **not** behavioral evidence. The formatted test-only [run 41604688393](https://github.com/TechWisard/desktop-pilot/actions/runs/41604688393) **confirmed the original bug on both Windows Server 2022 and 2025**: 400 tests passed and the new shared-root regression failed (the old matcher returned a Work profile ID instead of `None`). The unrelated distinct-root regression passed. The [corrected PR #154](https://github.com/TechWisard/desktop-pilot/pull/154) passed the full [Windows run 41605140479](https://github.com/TechWisard/desktop-pilot/actions/runs/41605140479) on both platforms, including formatting, tests, strict Clippy, build, native integration and publication guards; the PR was merged as [`7fac642`](https://github.com/TechWisard/desktop-pilot/commit/7fac64253aba905b855323c077ee49d7e9dff361).

The unresolved draft organizer PRs #98/#101 are not changed. Physical Windows 11 GUI validation at 100%–200% scaling, real launch-profile attribution, long application outages, mixed projects, and false-placement measurements remain outside this source-level check. No personal telemetry, network service, profile migration or automatic layout change is introduced.
