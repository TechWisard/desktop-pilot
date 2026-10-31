# A misleading automatic-review date after observation pause or clock correction — 2026-10-31

## User-facing risk

Desktop Pilot's Home page previously reported **"At the next idle opportunity"** for an Adaptive configuration whose usage observation was paused (`automatic=true`, `collect=false`). The actual `engine::automatic_review_with_boundary` refuses unattended placement while collection is paused. A recent worker heartbeat could therefore make the Home page promise a shortcut change that would never happen. Source inspection also identified the inverse case: after a Windows clock correction, `last_review` could be over a day in the future. The actual scheduler treats that as due immediately to avoid freezing adaptation, but Home displayed a date many weeks away.

These are source-confirmed inconsistencies, not reported Desktop Pilot user incidents or measured human delay. The existing [adaptive UI evidence](2026-10-26-adaptive-shortcuts.md) (Findlater/McGrenere 2004 and Gajos et al. 2008) motivates truthful, predictable adaptation; it does not establish how often the bug happens.

## Failure-first reproduction

`src/ui/clock_health_tests.rs::next_review_respects_paused_collection_and_corrected_clock` opens actual `Store`/SQLite and `Settings::new`, persists `Config`, `State` and the background-disabled flag, reloads the real Settings snapshot, and calls the same `next_review()` used by Home. The sequence covers pause, explicitly disabled background, a review checkpoint 45 days in the future, a normal recent checkpoint, and manual mode. No desktop files are moved.

The failure-first commit [ac231d4](https://github.com/TechWisard/desktop-pilot/commit/ac231d4e413b76968d84e045cb194b0ded1688e2) failed the precise paused-collection assertion on **both Windows Server 2022 and 2025** in [run 41169001740](https://github.com/TechWisard/desktop-pilot/actions/runs/41169001740) (Windows 2022: 340 passing tests and this one failing). It returned "At the next idle opportunity" where the test required the paused status.

## Minimal correction and limits

Home now checks the saved collection permission and explicitly disabled worker before reporting an automatic review. The displayed due-now decision reuses `engine::scheduled_review_due`, the **same production predicate** used by initial and final automatic placement eligibility, so backward clock corrections cannot leave the displayed schedule inconsistent with that predicate. No changes to actual placement cadence, learning records, shortcut ownership, user-authored rules, Undo, settings format, dependency list or telemetry.

Check this exact branch using the unchanged Windows 2022/2025 workflow: format, locked Rust suite, strict Clippy, locked build, native integration and release guards; then verify post-merge main. A synthetic real-path regression supports correctness, **not** real user-time savings. Still unverified: physical Windows 11 at 200% display scale, real disconnected network roots and multiday project-change usability.
