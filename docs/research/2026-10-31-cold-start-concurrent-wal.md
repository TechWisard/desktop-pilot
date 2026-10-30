# Cold-start readability during a concurrent manual edit (2026-10-31)

## Baseline and motivation

Inspect public `main` `9caf01e8c5d916638fb2f24d6655e95a89bd507e`, Desktop Pilot 0.2.69. The prior scan/large-catalog work is in `2026-10-27-catalog-snapshot.md`, `2026-10-28-idle-settings-snapshot.md`, and issue [#22](https://github.com/TechWisard/desktop-pilot/issues/22). No PR was open at the beginning of this work. Existing public Windows CI is operational and `finite-checks` is unnecessary.

A cold Settings window must be useful *before* another process finishes a concurrent profile write: last committed shortcuts and explicit rules must be searchable immediately; uncommitted rules must never appear as chosen. SQLite's [WAL documentation](https://www.sqlite.org/wal.html) explains that readers and one writer can operate concurrently, while [snapshot isolation](https://www.sqlite.org/isolation.html) explains why a new read sees committed state only. This describes the expected transaction behavior, not Desktop Pilot field performance. The external Flow Launcher custom-catalog report [#4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484) motivated earlier cold-start work but is **not** evidence of this app having a defect. Prior adaptive-presentation evidence is collected in `2026-10-26-adaptive-shortcuts.md`.

## Reproducible production-path acceptance

`src/ui/catalog_snapshot_tests.rs::cold_start_with_busy_writer_keeps_saved_catalog_and_refreshes_after_commit` reuses a real 5,001-item saved SQLite catalog, a valid manually chosen `.url` source, and a synthetic disconnected custom path. A second real Store connection holds `BEGIN IMMEDIATE` while writing an explicit `Never` decision but before committing it.

The regression creates an entirely new `Settings::new` instance during that writer transaction. It checks that the committed `Always` rule, ready-to-use recommendation, and search-by-note remain accessible with exactly one catalog read. After `COMMIT`, normal `Settings::reload` must show `Never`, remove the previous recommendation, and leave source shortcut bytes and desktop ownership untouched. No timings are converted into claims about real user time saved. No simplified selector or fake database is used.

This is a coverage extension, **not** a confirmed new production defect or product feature. A passing test warrants no new binary release. If the Windows test fails, diagnose its failure, then correct production code only with a second green run. Preserve current protected release checks.

## Evidence and limits

Run unchanged `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integrations, README and release guards on both Windows 2022/2025 CI runners. Capture the exact branch SHA and results before merge. Continue issue #22 with actual Windows 11/200% display, cold startup on a disconnected real network root, long-running habit changes and task-level latency measurements. This test checks concurrent committed state and UI model logic, not a physical GUI or human time-to-first-action.
