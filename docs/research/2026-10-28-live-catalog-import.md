# Prompt recognition after a manual catalog import — 2026-10-28

## Starting point and evidence

Baseline: publicly released Desktop Pilot **0.2.28**, `main` commit `a06d71e44bcdc93922e48368aaa0d126d724310b`. The previous asynchronous-scanner fix remains in place and was verified in [the nonblocking-scan report](2026-10-28-nonblocking-catalog-scan.md). At investigation start there were no open PRs and issue [#22](https://github.com/TechWisard/desktop-pilot/issues/22) was open. This work continues that investigation; it does not repeat the earlier large-catalog search or Windows-version matrix.

Source-confirmed behavior: `Settings::add_shortcuts` writes imported applications through its own SQLite connection and returns a successful result without posting a worker command. The running `engine::monitor` loaded `Store::apps` only at startup, after an explicit queued command, or after the periodic scanner completed. With a quiet existing catalog, that next scanner could be six hours away. The Settings window immediately displayed the imported app, while the background foreground-matching cache remained stale. This is a concrete inconsistency, not an estimate of a user's perceived delay.

The existing primary study by Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces*, CHI 2008 (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), associates accurate adaptive predictions with improved utilization and performance. A separate user request [PowerToys #47478](https://github.com/microsoft/PowerToys/issues/47478), opened April 30, 2026, describes frustration when a quick-access panel is not directly controllable. Neither source tested Desktop Pilot; they motivate respecting deliberate manual choices and avoiding a confusing gap between importing and recognition.

## Failure-first reproduction on production paths

`src/engine/live_import_tests.rs` starts the real COM-initialized `monitor` and real asynchronous scanner on an isolated profile, waits until the initial scan is complete, then uses a separate real SQLite connection to import a valid `.url` entry. A test-only observer reads the monitor's **actual in-memory application cache** at the end of each poll; no independent surrogate ranker, mocked import database, network drive or executable launch is used. The test supplies the same catalog revision signal that a real import must produce.

`src/ui/import_ownership_tests.rs` independently invokes the actual asynchronous Settings import and checks that a changed import commits exactly one revision while reimporting an unchanged managed quick copy commits none. These two tests exercise both sides of the notification contract.

[Baseline Windows run 40610581497](https://github.com/TechWisard/desktop-pilot/actions/runs/40610581497) finished with **247 passed and two failed tests** on each of the two existing Windows runners. Failure one: after a successful initial scan the background cache did not acquire the new shortcut within the seven-second test window. Failure two: real Settings import did not publish a catalog revision. The production code compiled; formatting and earlier release safeguards passed before the deliberate behavior failures. No external application was launched and the original file remained untouched.

## Correction and bounded comparison

- The Settings import transaction writes a monotonically advancing `catalog_epoch` value **once per changed batch** in the existing local `kv` table. Fully unchanged imports and managed-copy reimports do not update the marker or app row. A failed transaction commits neither the catalog nor its revision.
- The background monitor reads the current revision every normal poll and reloads `Store::apps` only when the value changes. Startup reads revision *before* its initial catalog snapshot so an import between those reads cannot be permanently missed. An import racing the subsequent reload can at worst cause a harmless extra read on the next tick.
- The periodic asynchronous disk scan remains independent, and the pre-existing scan-versus-import source guard still prevents a stale scan from replacing a newer user-selected source. User commands, history, manual rules, originals and Undo are unchanged.

[Corrected Windows run 40611142481](https://github.com/TechWisard/desktop-pilot/actions/runs/40611142481) passed the full suite: **249 Rust tests, 28 native integration checks and nine publication guards on each runner**, alongside README verification, formatting, warnings-denied Clippy and a locked build. The new monitor test now observes the imported entry within its seven-second deadline after the initial scan. A batch signals once; a managed reimport signals zero times. No placement or source file changed in this test.

The six-hour interval is a source-control boundary, **not** a measured before latency. The seven-second test limit is a deterministic acceptance deadline, **not** a measured human-time or energy saving. We have not benchmarked physical Windows 11, 200% DPI input, network latency, or real-user adaptation.

## Continuation

Keep [#22](https://github.com/TechWisard/desktop-pilot/issues/22) open for physical Windows 11 time to first searchable saved result at 100 / 1,000 / 5,000 entries; simultaneous slow network and cloud-sync sources; removed profile drive; explicit recovery affordances; and multiday changing-project sequences. Measure false placements, extra movement, adaptation delay, explanations, actual user actions and recoverability. Avoid creating extra complexity or shipping unfinished scenarios merely to increment a version.
