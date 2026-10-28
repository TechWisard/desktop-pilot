# Stable Settings refresh without repeated catalog parsing (2026-10-28)

## Starting point and motivation

Begin from publicly shipped Desktop Pilot 0.2.39, main commit `00c2d547d174c9231bd123f95ff8a0d22bef577c`, after the idle learning writer fix of PR #41. No open pull requests at inspection. The separate public `TechWisard/finite-checks` repository was not found; the existing public desktop-pilot repository already has working Windows 2022/2025 CI. No separate stand or private build transport was needed.

The existing Settings UI calls `reload()` every two seconds whenever no job is running. At this revision, every call unconditionally opens a SQLite read snapshot, deserializes all saved apps, loads 90 days of activity, recomputes ranking and preview, sorts the catalog and rebuilds the full-text search cache. These are source-confirmed operations; the cost to a user's interactive frame was not measured on physical Windows 11. For 1,000–5,000 saved entries, redundant parsing during idle is a plausible source of unwanted stalls, distinct from startup search and bulk import already addressed in 0.2.38.

## Primary sources and decision boundaries

- SQLite [PRAGMA data_version](https://www.sqlite.org/pragma.html#pragma_data_version) is explicitly intended for programs maintaining in-memory or displayed database content. Its value changes after *another connection* commits, not after writes on the same connection. For correctness, pair it with same-connection `SELECT total_changes()` and only compare values collected on the same long-lived Settings connection. Neither is a persistent cache.
- Previous user-centered research: Findlater & McGrenere (CHI 2004), [DOI 10.1145/985692.985704](https://doi.org/10.1145/985692.985704), establishes that human control over adaptive presentation is important. It does not provide latency numbers for Desktop Pilot. Keeping manual anchors and current feedback immediate is a requirement, not a conclusion from timing benchmarks.
- The independent Flow Launcher custom-catalog report #4484, already reviewed in issue #22, is a motivation for testing saved catalog availability; it cannot establish any Desktop Pilot defect or current Flow Launcher regression. This work avoids repeating that search.

## Correction

Use a volatile checkpoint of `data_version`, connection-local `total_changes()`, and monotonic elapsed time. With both SQLite indicators unchanged and the last full refresh less than 60 seconds old, avoid re-reading and sorting the saved app catalog. Always check the native worker singleton so an unexpectedly terminated background process still shows as stopped. After any external/local commit or elapsed deadline, execute the unchanged complete, transaction-consistent reload; a failed reload discards the checkpoint. The one-minute deadline also bounds staleness of time-decayed scores and clock adjustments.

The choice is deliberately conservative: a live background session may still commit frequently, which causes a normal full refresh. No new tables, triggers, persisted cache, timers, dependencies, telemetry, threads, settings or altered selection thresholds.

## Executable verification

The tests live in `src/ui/catalog_snapshot_tests.rs` and use `Settings::new`, `Settings::reload`, real `Store::open`, SQLite/URL fixtures, UI search cache, and the existing test-only `CATALOG_READS` counter. They cover 100/1,000/5,000 saved entries with an intentionally unavailable optional drive; 12 consecutive unchanged polls; another SQLite connection replacing a manual rule; current-connection rule edits; worker status and a queued command reaching `done`; forced full refresh and the 60-second monotonic expiry. No simplified ranking model replaces the production selection.

Run unchanged CI gates: `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `./scripts/test-native.ps1`, `./scripts/test-release-workflow.ps1`, and README checks. Failure-first [Windows 2022/2025 run 40732557377](https://github.com/TechWisard/desktop-pilot/actions/runs/40732557377) used unchanged 0.2.39 production code and passed formatting. 275 tests passed, while two new regressions failed: 12 catalog reads rather than zero on 12 unchanged polls, and four reads rather than one after three unchanged polls plus an external rule change. The earlier run 40732339818 stopped at formatting and is not behavioral evidence. Corrected branch/PR and main release CI remain necessary; only a successful release job can publish the version.

## Limits and continuation

The test counter measures actual catalog reads, not a proven reduction in task time, real UI frame latency, CPU power or battery use. Screenshots and physical input at 200% scale, prolonged network/profile disk outages, real cold OS startup, longer combinations of project/habit changes and user comprehension of explanations remain open in issue #22. Preserve all choices, sources, on-desktop ownership and Undo during future work.
