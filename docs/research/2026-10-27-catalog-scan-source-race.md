# Source ownership across long catalog rescans — 2026-10-27

## Why this scenario matters

Continue from the released 0.2.25 (main 4bd928af1e00ff2070c3fd7b41dedc1e38bb36a7) and the saved investigations of catalog snapshots, no-op rescans and writer contention. No open PR existed at inspection. The previously cited Findlater/McGrenere CHI 2004 and Gajos et al. CHI 2008 findings support keeping familiar placement and explicit user intentions stable; they do not quantify this product's benefit. SQLite's [WAL concurrency guide](https://www.sqlite.org/wal.html) describes concurrent readers and a single writer. An independent [Flow Launcher report #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484) describes slow availability of saved custom shortcuts after restart; it motivates examining longer scans but is not evidence of a Desktop Pilot race.

## Source-confirmed interleaving

The production refresh loads a catalog into memory, checks shortcut files and enumerates roots, then writes the computed rows. The importer can complete a different SQLite transaction during the file scan. At commit, refresh preserves the latest policy, seed, and note, but not a newer explicitly imported **source path**. Consequently a stale scan can silently overwrite an original source selected by the user. A concurrent import for an app first discovered during the scan is vulnerable as well. The initial unfinished-transaction guard is also outside the scanning interval; placement can become prepared after the initial check.

## Real-code reproduction

The test-only hook executes a user-source update on the actual SQLite Store between real shortcut parsing/enumeration and the production refresh commit; it does not implement a separate ranking, scan or persistence model. Scenarios include a managed user-pinned quick copy and Undo, import of a new entry also seen on the desktop, and an interrupted placement appearing during the scan. Compare source, manual Always/Never, days, state, copy bytes, preview/Undo, and absence of unintended changes. A test-only hook exists exclusively in test builds.

## Intended correction

Preserve source changes made since the refresh snapshot: skip obsolete scanner rows when their saved source changed or when an app was concurrently inserted after the snapshot. Repeat the unfinished-operation guard inside the final transaction. Subsequent rescans of the new original still update verified metadata; stable rescans should not increase SQLite writes. Do not change ranking, history, manual choices, layout controls, network behavior or telemetry.

## First-use documentation

Issue [#27](https://github.com/TechWisard/desktop-pilot/issues/27) documents obsolete filenames in both README installation steps. Use a stable latest-release archive pattern with a scripted bilingual regression that checks the first step, binary and SHA-256 guidance; preserve the historical release notes. The source README and copies packaged into releases must agree.

## Validation boundaries

CI is a controlled Windows Server runner and deterministic interleaving, not a physical Windows 11 desktop, multiweek human habit study, or measured task-time saving. Existing catalog, metadata, import, desktop-copy, rollback and release checks remain mandatory. Record test failures on unchanged production logic before applying the fix, then verify the entire unchanged CI suite. No release should be published unless the final merge's CI and public archive checks pass.

## Reproduced before/after on production paths

The new cases must fail against released 0.2.25 but pass against 0.2.26 while exercising the real \`refresh_apps\` and SQLite transaction. In an interleaved source import, the selected source, Always/Never, original bytes and managed-copy state must not be overwritten by stale filesystem findings. In an interrupted placement, no scan write may occur after a new prepared operation. Stable rescans must still be no-ops and metadata updates must still succeed. Runner execution evidence belongs in a separate validation record; neither elapsed seconds nor real-user time savings are inferred.

## Executed CI evidence

The formatted test-only baseline on released 0.2.25 is [run 40505861885](https://github.com/TechWisard/desktop-pilot/actions/runs/40505861885). Both existing Windows Server runners passed the bilingual installation-content check, publication guards and formatting, then ran the real unchanged catalog refresh: **239 Rust tests passed, 3 new interleaving regressions failed**. Those failures demonstrate source overwrite for both existing and first-time imports, and failure to refuse a newly prepared placement. Earlier unformatted authoring runs are not evidence of production behavior.

The corrected candidate first passed **242 Rust tests** on both runners ([run 40505866235](https://github.com/TechWisard/desktop-pilot/actions/runs/40505866235)), but strict Clippy rejected the nested test-only hook type; this was corrected by naming the type, without disabling the lint. The final checked candidate [run 40506420118](https://github.com/TechWisard/desktop-pilot/actions/runs/40506420118) passed unchanged Windows Server 2022 and 2025 jobs: **242 Rust tests, 28 native integration checks and 9 publication guards per runner**, with formatting, warnings-denied Clippy, locked build and the bilingual README guard. A full validation record is in [catalog-scan-source-race-validation.json](2026-10-27-catalog-scan-source-race-validation.json).

No stopwatch or user telemetry was collected: the reproduced benefit is preventing three documented incorrect state transitions. Final main/tag/release-public-download verification is separate and must not be inferred from candidate CI.
