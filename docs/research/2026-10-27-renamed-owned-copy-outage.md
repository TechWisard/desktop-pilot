# Managed quick-copy identity after rename, source outage and replacement — 2026-10-27

## Starting point and independent context

Start from published 0.2.26, main `19772ad147223b96da8954bc7ab27fb4bf3e0293`. Windows checks and release/public download verification had passed; no PR was open. This extends the existing manual-ownership and source-alias investigations without redoing the research catalog. Findlater/McGrenere, *Static, Adaptive, and Adaptable Menus* (CHI 2004), and Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces* (CHI 2008), motivate respecting spatial expectations and explicit choices; neither measured Desktop Pilot. Original sources: https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/ and https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml . A Flow Launcher user described delayed availability of a separate shortcut library after restart (https://github.com/Flow-Launcher/Flow.Launcher/issues/4484); that report is not evidence for the bug identified here and its symptoms were reported resolved in June 2026.

## Confirmed source defect and failure-first Windows reproduction

`refresh_apps` filters a managed quick shortcut from file discovery by its **saved path**. If the user renames it before reconciliation, that path is stale. When the original file is then temporarily unavailable, the scanner accepts the renamed quick copy as a readable alias, replaces the catalog's original source with its own managed copy, and may make the manual rule/Undo inconsistent. If the renamed copy's target was edited, discovery instead imports a second automatic catalog entry from a file the user modified. Neither case is an explicit new-source decision by the user.

Failure-first [Windows CI run 40543195730](https://github.com/TechWisard/desktop-pilot/actions/runs/40543195730) added only real-code regression tests to the unchanged 0.2.26 implementation. On both Windows Server 2022 and 2025, `cargo fmt --check` passed, **243 tests passed, two new tests failed** (incorrect original-source substitution; duplicate app discovery). The independent byte-identical original-alias control passed. The initial test-authoring run 40542879532 failed formatting and is not behavior evidence.

## Correction and safety limits

For up to eight managed desktop copies, inspect the saved filesystem identity. If a saved path no longer names its managed file, skip any rediscovered shortcut with that same file ID, including renamed/edited copies. When all managed paths still hold their original files, regular catalog scans perform no additional per-candidate identity checks. If a displaced-copy identity cannot be verified, skip that candidate with a warning instead of treating it as an original. Genuine independent duplicate links with distinct file IDs remain eligible for existing source-alias recovery. No source files are rewritten; manual Always/Never, history, Undo, selection ranking, SQLite schema, settings, external services and telemetry are untouched.

A fourth regression covers the less common case where a renamed managed copy's previous pathname was replaced by a different user file. Tests use real `Store`, Windows `.url` reads, actual `refresh_apps`, `reconcile_manual`, `preview`, `apply`, and `command`, not an isolated ranking model.

## Acceptance, verification and remaining work

Run the unchanged Windows 2022/2025 workflow: formatting, locked Rust tests, warnings-denied Clippy, locked build, bilingual README guard, 28 native integration checks, and nine release-publication guards. Verify merge CI, release job, tag/commit match, public binary and source archive hashes, executable ProductVersion, and licenses before declaring release. Do not equate synthetic correctness or runner timings with human time savings.

Existing profiles whose saved managed copies lack a stable `file_id` cannot recover arbitrary renames by identity alone. Physical Windows 11 Explorer, offline cloud-sync cases, 200% DPI input, custom-root slow scans, multiweek habit changes and user-time measurement remain in issue #22. This fix is about preserving explicit source identity and stability, not claiming an optimal adaptive algorithm.

## Verified candidate checks

The corrected versioned candidate `e654512dc733ac758dee1595807d14c216512d55` passed [unchanged CI run 40544229621](https://github.com/TechWisard/desktop-pilot/actions/runs/40544229621) on **both** Windows Server 2022 and Windows Server 2025: 246 Rust tests passed, zero failed, including all four new production-path cases; 28 native checks, nine publication guards, `cargo fmt --check`, strict warnings-as-errors Clippy, locked build and bilingual README validation passed on each runner. This is a correctness check against synthetic Windows shortcut/SQLite profiles, not an observed improvement in actual people's task time. Subsequent PR/main/tag/public-download verification is independent. The exact before/after results are recorded in [renamed-owned-copy-outage-validation.json](2026-10-27-renamed-owned-copy-outage-validation.json).

