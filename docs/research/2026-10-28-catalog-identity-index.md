# Linear-time catalog identity matching — 2026-10-28

## Why this matters

Baseline: published v0.2.30, `fa4a27509adb0c904e9ab460e844b2ff9b9ac723`. At the start of this investigation, main CI and the release-download verification were successful, PRs were closed, and issue [#22](https://github.com/TechWisard/desktop-pilot/issues/22) remained open. This continues the same large-catalog investigation without repeating its earlier UI startup and SQLite fixes.

Source-confirmed performance defect: `engine::refresh_apps_guarded` walked discovered .lnk/.url paths and used `known.iter().position(|a| a.identity() == app.identity())` for every one. Each of those calls rebuilt a launch identity; on a large saved catalog this was a quadratic identity-matching pass on top of legitimate filesystem reads. Existing unchanged-source SQLite write suppression and the nonblocking scanner from 0.2.28 do not eliminate that CPU work.

Primary HCI context remains Findlater and McGrenere, [CHI 2004](https://doi.org/10.1145/985692.985704), and Gajos et al., [CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml): predictable placement and clear user control matter alongside adaptive accuracy. The independent [Flow Launcher custom-source report #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), opened May 21, 2026 and closed June 8 after an apparent plugin update, motivates testing large saved catalogs. It is **not** evidence that Desktop Pilot had that product's same behavior or user-observed delay.

## Small production change

Build one map from complete launch identity to the first catalog index, then use it while discovering shortcuts. Newly encountered identities enter the same map. Choosing the first index preserves `Vec::position`'s behavior for old profiles with duplicate logical identities. A saved app's identity is not changed during the scan's non-identity metadata refresh. Alias handling, full launch arguments, manually imposed Never/Always, offline sources and the stale-scan commit guard remain in their original production paths.

The new lookup eliminates a per-discovered-item scan of the entire saved list. It does **not** reduce actual shortcut file reads, change learned ranking weights or invent a cache invalidation rule. No database schema, dependency, user setting, external call or telemetry is added. Test-only instrumentation counts `App::identity` evaluations on the actual Rust path; the production build has no such counter.

## Reproducible acceptance evidence

New `src/engine/identity_index_tests.rs` uses real Windows internet shortcuts, the production scanner, a saved SQLite catalog of 100 and 1,000 distinct targets, explicit Always/Never, usage history, manual-mode Apply, unchanged quick-copy bytes, no-churn preview and real Undo. The test resets a thread-local identity counter immediately before the second scan and requires at most `10 * N + 100` identity evaluations (1,100 for N=100 and 10,100 for N=1,000). The old `Vec::position` implementation made up to N matching comparisons per discovered file; the new index performs one lookup per file and builds the index once. The bound is deliberately loose enough for legitimate native parsing, but excludes the quadratic design.

Branch [Windows CI 40633937947](https://github.com/TechWisard/desktop-pilot/actions/runs/40633937947) passed on both Windows Server 2022 and 2025: 254 Rust tests, 28 native integration checks, nine publication guards, README checks, `cargo fmt --check`, strict `cargo clippy --locked --all-targets -- -D warnings` and `cargo build --locked`. An earlier iteration [40633821956](https://github.com/TechWisard/desktop-pilot/actions/runs/40633821956) failed only the test file's formatting, which was corrected without changing the assertions. The test-only baseline branch run was cancelled before completing, so there is no claim of an independently executed red baseline.

**Interpretation:** source-level comparison shows quadratic-to-linear identity matching, and the passing bounded counter checks the corrected production scanner. This does not measure time savings for people, prove a specific CPU percentage or benchmark real network delays. The test prints elapsed milliseconds only if executed with `--nocapture`; CI runner wall-clock timing is not a physical Windows 11 desktop measurement.

## Continuation

Issue #22 remains open for physical Windows 11 time-to-first-saved-search/preview on 100, 1,000 and 5,000 entries at 100%/200% DPI, disconnected network/cloud paths and full profile-disk absence. Keep mixed Work/Personal/rare-app and weekend/project-change sequences, with false placements, review churn, preference survival, recovery and Undo as acceptance measures. A separate PR/main/release check is required before shipping; never treat a branch check as confirmation of a public archive.
