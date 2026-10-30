# Newer same-source imports must win over stale catalog scans (2026-10-30)

## Starting point and evidence

Baseline: released Desktop Pilot 0.2.62, with test-only main commit `6f89f926ed88494e7da878f56486561c4af148df`. Another independent operating-mode PR (#68) was open; this work uses a separate branch and does not change that PR, repository visibility or the standard CI.

Existing evidence-based UX background, without repeating external searches: Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004), https://doi.org/10.1145/985692.985704. Their findings motivate predictable explicit choices but do not quantify Desktop Pilot productivity. Previously documented analogous launcher freshness reports and the established scanner/ownership tests are in [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22), `docs/research/2026-10-28-concurrent-placement-scan.md` and `docs/research/2026-10-30-late-manual-rescan.md`.

## Confirmed product defect

The source scanner takes a snapshot of saved shortcuts, reads their source files and commits results later. Its commit-time concurrency guard previously checked only whether a row's original *path* had changed, plus preserving the newer manual policy, note and historical score. A user could explicitly reimport the **same original path** with a changed icon while the scanner was in flight. Because the path and launch ID were unchanged, the late scanner overwrote the newly imported icon with obsolete metadata. The visible catalog then disagreed with the user's latest import until another rescan.

The deterministic test `engine::catalog_race_tests::same_source_manual_reimport_cannot_lose_newer_metadata_to_stale_scan` uses real native `.url` decoding, a SQLite Store, a manually protected quick copy, usage history, actual apply/Undo and the real `refresh_apps` commit gate. After the old scan reads `C:\\Icons\\old.ico`, the source is updated to `C:\\Icons\\new.ico`; the persisted row and `catalog_epoch` are updated exactly as a Settings reimport would do. The delayed scan then commits.

The initial test-only CI failed formatting; that run is *not* behavioral evidence. Formatted [failure-first Windows run 41012530163](https://github.com/TechWisard/desktop-pilot/actions/runs/41012530163) reached Rust tests on both Windows Server 2022 and 2025: **312 existing tests passed, the new regression failed on each runner**, with persisted old icon against verified new icon. This is a controlled race, not a reported real-user incidence.

## Correction and protective coverage

The in-memory scanner snapshot now retains original source-derived catalog fields alongside each saved source. Inside the existing SQLite writer transaction, it skips only entries whose current persisted source-derived data has changed since enumeration; this includes same-path icon/name/launch/validity changes. A separate policy or note edit does not block a legitimate scanner metadata update, and existing policy/history merging remains unchanged. A second regression exercises a concurrent Never rule while a verified icon refresh proceeds.

The snapshot is transient, in-process and linear in catalog size. No new persistent index, background service, dependency, schema, setting, telemetry or source-file mutation. Unchanged source-row writes remain skipped as before; the 5,000-file catalog and manual ownership/Undo regression suites remain mandatory.

## Acceptance gates and limitations

Require normal `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, README tests, nine publication guards and 29 native checks on both existing Windows GitHub runners, followed by PR checks and release executable/package/tag/main verification if a new version is published. Keep the new test and its baseline red run as reproducible evidence. No human time savings, physical Windows 11 or real network-drive performance measurements are inferred.

Remaining work: [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22) (physical Windows 11/200% DPI, true cold process-start to first result, disconnected network/cloud sources and long-duration project patterns) and [issue #56](https://github.com/TechWisard/desktop-pilot/issues/56) (real low-level input callback/CPU measurements). Do not repeat completed baseline investigations.
