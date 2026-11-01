# Recheck user activity after slow automatic-review preflight — 2026-11-01

## Evidence and scenario

Continue the established [adaptive-interface research](2026-10-26-adaptive-shortcuts.md) and the [earlier quiet-permission correction](2026-11-01-automatic-review-quiet-recheck.md), without repeating the literature search. Gajos et al., CHI 2008, measured the value of predictable, accurate adaptive UI (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). A February 2026 Fences report describes unexpected icon rearrangement (https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop); this is **reported behavior of a different product**, not measured incidence in Desktop Pilot.

**Confirmed in the production source:** the previous quiet recheck ran after the changes mutex but **before** `apply_locked`. That function then rescanned the catalog, reconciled existing shortcuts, computed ranking and layout, and reserved positions. An active user returning during any of those potentially slow steps would not be detected. Both source parsing and layout preflight can be substantially more expensive with many shortcuts. The gap follows directly from the call order; its frequency in real installations has not been measured.

## Correction and regression

The shared placement transaction now accepts a last-minute quiet predicate. Explicit manual Apply always passes a permissive predicate. Automatic review keeps the existing early idle check and checks **again after catalog refresh, reconciliation, preview and position reservation**, just before the first state-checkpoint write or history/staged desktop file creation. A failed check returns a deferred outcome, not an error, so the automatic review remains due for the next quiet maintenance pass. The existing changes mutex, manual anchors, source validation, atomic journal/rollback and user settings are preserved. No polling, network calls, telemetry or new visible controls are added.

`src/engine/idle_review_tests.rs::active_user_during_slow_preflight_defers_the_same_due_review_without_churn` drives the production review path with actual SQLite, a native-parsed `.url` file, catalog refresh, selector and placement. It injects the return to activity only **at the post-preflight boundary**: both quiet checks occur, the first passes and the second fails. It verifies that the original source bytes, managed state, review timestamp and desktop files remain unchanged. On a subsequent quiet attempt, the due review performs exactly one placement and an immediate repeat is a no-op. The timing transition is synthetic; the application storage and file operations are real.

## Verification and limitations

Require locked Rust unit/integration tests, formatting, warnings-denied Clippy, locked build, native Windows fixtures and release guards on Windows Server 2022 and 2025 before integration. This does **not** eliminate a user input arriving midway through the filesystem transaction; making mid-commit cancellation safe would require a separate design and rollback acceptance work. Physical Windows 11, slow removable/network sources, high-DPI UI and real human productivity remain untested by this synthetic scenario. Do not infer measured time savings.
