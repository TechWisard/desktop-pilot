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
