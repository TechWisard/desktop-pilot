# Managed shortcut continuity across a corrected midnight — 2026-10-31

## Why this case matters

Baseline: public `main` `a15fb48e98356f6b095b5b731e4b975554305f91`, Desktop Pilot 0.2.74; no open pull requests. The ordinary public Windows checks work, so an additional test repository is unnecessary.

The earlier [clock correction analysis](2026-10-31-recent-rewind-seed-stability.md) protected an already managed shortcut's **seed** when Windows time moves backwards. That does not cover recent **per-day session history**: the current local date is a SQLite bucket stamped at midnight, which can become *future-dated* when Windows is corrected back across midnight. `rank_with_owned` discarded every such day, so it could score an otherwise familiar managed shortcut at zero and recommend a replacement. This is a source-level hypothesis tested below, not an independently observed end-user report.

[Microsoft System Time](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) documents that system time can be adjusted. [Gajos et al., CHI 2008](https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/) provides primary research on accuracy and predictability of adaptive interfaces. Its findings motivate the acceptance criterion; they are not Desktop Pilot usage measurements. Research on cold-start catalog discoverability and external reports from other launchers is already collected in [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22), so that search was not repeated.

## Failure-first reproduction

`src/model/selection_clock_tests.rs::short_clock_rewind_across_midnight_keeps_managed_usage_but_not_new_candidates` exercises the actual native `.url` parser, persisted Store/SQLite days, current `rank_with_owned`, and production `engine::preview_loaded` selection with a one-slot desktop. An existing Auto shortcut has 12 saved sessions and 3600 recorded seconds in a local-day bucket two hours after the corrected clock; the alternative has a valid recent historical score of 18. The existing copy is marked managed but not manually protected. Original and copied files remain untouched.

On the unchanged production selector, the new test reproduced **one unnecessary proposed replacement**. [Windows CI 41185718406](https://github.com/TechWisard/desktop-pilot/actions/runs/41185718406), test-only branch `0415f7b641010b765404ac535d98045b63e74b91`: formatting and repository guards passed, 342 Rust tests passed and this one failed on *both* Windows Server 2022 and Windows Server 2025. The initial formatting-only failure in the preceding run is not behavioral evidence.

## Minimal correction and safeguards

Keep a saved day no more than 24 hours ahead of the corrected clock **only if that app already has a managed quick copy**. Clamp its decay age to zero, instead of granting a future timestamp extra weight. For unowned apps, genuinely distant future days and explicit `Never` policies, preserve the previous strict behavior. No additional user setting, telemetry, background work, filesystem mutation, or dependency is introduced.

Positive and negative controls in the same regression require the managed copy to remain selected, a new unowned candidate to receive no benefit from future history, manual exclusion to prevail, and source/copy bytes to remain intact. Compare only the synthetic one-slot recommendation (one proposed unwanted change before vs zero after); do not claim this is a measured improvement in users' time or proof of Windows 11 physical UI behavior.

## Validation / continuation

Run unchanged format, locked tests, warnings-denied Clippy, locked build, native integration fixtures and publication checks on both Windows runners. Verify branch head and workflow before merge. Validate merged `main` separately and publish a binary only when release guards and artifacts are confirmed. Still outstanding: real Windows 11 at 200% scale, disconnected network sources, long-term habit shifts, and task-level first-use measurements.
