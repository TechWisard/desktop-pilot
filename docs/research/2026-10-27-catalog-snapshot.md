# Reusing the saved catalog on Settings cold start — 2026-10-27

## Evidence and scope

Continue from v0.2.21, main commit `74a4d810a64357d7204f839daf3bc4f3a88c454b`. Issue [#22](https://github.com/TechWisard/desktop-pilot/issues/22) tracks cold-start search with large custom catalogs. An independent [Flow Launcher report #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), opened 2026-05-21, describes saved custom shortcuts taking up to 20 minutes to become searchable after a restart. It is not a Desktop Pilot measurement or proof of a corresponding bug. The [Gajos et al. CHI 2008 study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) supports predictable, accurate adaptive interfaces; it does not measure this implementation. Earlier literature is retained in `2026-10-26-adaptive-shortcuts.md`.

The v0.2.21 source itself reveals a narrower, confirmed inefficiency. `Settings::reload` performs `Store::apps()` to render the catalog and then calls `engine::preview`, which executes a second `Store::apps()` on the same SQLite read snapshot and parses every catalog row from JSON again. The success path also obtains the history through preview, while an invalid-preview fallback queries history separately. This is repeated on each regular Settings reload, not only at first installation. The duplicate reads are **source-confirmed**; their wall-clock impact on actual Windows 11 hardware remains unmeasured.

## Narrow change and invariants

`engine::preview_loaded` consumes an already-read configuration, app list, history, state and one evaluation timestamp. Both the standalone public preview and Settings now invoke this same real ranking/selection implementation. Settings loads one catalog and one history snapshot, calculates selection **before** sorting by display name, then builds its searchable UI. Selection previously received the ID-ordered list from SQLite; sorting it first would have changed the order in which equal-priority manual anchors were appended. No scoring weights, source-file checks, manual rules, undo journal, schema, telemetry or Windows settings are changed.

On the normal Settings startup/reload path, the number of full `Store::apps` calls decreases from **2 to 1**. This is a deterministic count of actual catalog reads, not an asserted 50% improvement in latency, CPU consumption or disk I/O.

## Reproducible production-path tests

`src/ui/catalog_snapshot_tests.rs` seeds real SQLite profiles with 100, 1,000 and 5,000 saved catalog entries; one original is a valid parsed `.url` and the remaining synthetic catalog records represent already indexed shortcuts on an unavailable optional drive. It opens the real `Settings::new`, queries the actual `CatalogCache`, checks the existing `engine::preview` for exactly matching selected/added/removed apps and ranking eligibility, then reloads Settings. A thread-local **test-only** counter in `Store::apps` checks that each Settings startup and reload performs exactly one complete catalog read. Separate invalid-settings coverage confirms that preview errors do not hide the saved catalog or bypass the count. Explicit Always/Never, distinct profile arguments, available originals, unavailable saved sources, and unchanged desktop ownership are represented.

This fixture tests visibility immediately after Settings loads; it does **not** simulate a physical Windows Explorer session, actual D: network share availability, shell clicks, a 20-minute indexing delay, or human productivity. It does not assert CI wall-clock timing thresholds because hosted runner timing has uncontrolled variance. Before/after comparison is the unmodified previous call path (two `Store::apps` invocations) versus the corrected call path (one), reinforced by the test-only call count. Existing native integration and UI scaling tests remain mandatory.

## Verification instructions and continuation

Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `scripts/test-native.ps1` and `scripts/test-release-workflow.ps1` on the unchanged Windows 2022/2025 CI. Before publishing, verify the main merge, versioned tag, downloadable binary/source archives, licenses and SHA256 sums.

Still open in issue #22: reliable user-input latency and time-to-first-result metrics, worker rescan under an interrupted/cloud-locked large library, actual Windows 11 display at 200% scale, and multi-week habit changes. No user-experience benefit beyond eliminating redundant parsing is claimed without measurement.

## Executed before/after evidence

The immutable [baseline test run 40475164510](https://github.com/TechWisard/desktop-pilot/actions/runs/40475164510), starting from released v0.2.21 with only the diagnostic counter and test fixtures attached, completed on Windows Server 2022 and 2025: **228 existing tests passed, 2 new assertions failed** because `Settings::new` made **2** full `Store::apps` calls instead of the desired **1**. Formatting passed; this is a behavioral red test, not a compilation or test-fixture error.

The [corrected branch run 40474998997](https://github.com/TechWisard/desktop-pilot/actions/runs/40474998997) completed on both Windows runners: **230 Rust tests passed, zero failed**, including both new actual-Settings tests across 100/1000/5000 entries; **28 native checks** and **9 publication guards** passed per runner, plus `cargo fmt --check`, strict Clippy and locked build. Exactly **1** catalog read was observed on open and reload. The structured result is in [catalog-snapshot-validation.json](2026-10-27-catalog-snapshot-validation.json). These are synthetic catalog fixtures running production UI/SQLite logic; no CI time number is interpreted as user-perceived acceleration. Final PR and release verification remain separate.
