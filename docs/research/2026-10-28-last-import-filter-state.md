# The "Last import" filter must follow the last selected shortcuts (2026-10-28)

## User problem, existing research, and source evidence

A user may import a group of shortcuts, select a second group, then reimport a previously known shortcut or an existing managed desktop copy. "Last import" promises to display the latest selection, including files already in the catalog. Showing the previous group instead is misleading: the UI appears to disregard an explicit user action, even when the import notice correctly says no database entries changed.

The existing [adaptive-interface research](2026-10-26-adaptive-shortcuts.md) and [idle Settings refresh investigation](2026-10-28-idle-settings-snapshot.md) are sufficient context. Findlater and McGrenere (CHI 2004) emphasize the value of predictable user-controlled interfaces; this is design motivation, not empirical evidence of a Desktop Pilot bug. The actual bug is established by source inspection and a failing production-path regression, not by an external anecdote.

At released 0.2.40 (`a642508f343534c06183df10a9069d6361dee7f4`), `CatalogCache::update` reused its index whenever query text, filter and sorting were unchanged, although filter 5 also depends on `Settings::highlighted_apps`. `Settings::poll_jobs` changes that set on every successful import, including no-op imports and managed quick copies. No-op imports correctly avoid bumping SQLite's `catalog_epoch`; the optimized unchanged-profile reload also correctly skips rebuilding the entire catalog. The combination leaves the cached "Last import" results from the previous selection.

## Failure-first reproduction

The new `src/ui/import_last_filter_tests.rs` test exercises actual asynchronous `Settings::add_shortcuts`, native `.url` parsing, SQLite, `Settings::poll_jobs`, the same catalog filter used by the Apps page, and real placement Undo, in an isolated Windows profile.

Sequence: import Project One; import Project Two; search "Last import"; reimport Project One without any SQLite catalog revision; then reimport an existing managed quick copy without a revision. Verify each displayed identity, manual Always rule, original file bytes, saved usage history, ownership, preview stability, and Undo.

[Failure-first Windows CI 40737131545](https://github.com/TechWisard/desktop-pilot/actions/runs/40737131545) passed formatting, then failed the **same new regression** on Windows Server 2022 and 2025: 278 other Rust tests passed on each, while the UI continued to show Project Two after reimporting Project One. An earlier attempt stopped at rustfmt before tests; it is not behavioral evidence.

## Minimal correction and verification contract

On a successful nonempty import selection, invalidate `CatalogCache::query` before switching to "Last import". The next Apps-page update recomputes only the in-memory filtered indices with the new selected-ID set. This does **not** force an unchanged SQLite catalog read, change the persisted revision, or weaken the 0.2.40 idle-refresh optimization. No new settings, dependencies, telemetry, ranking, shortcut placement or persistent state are required.

The red-then-green test and all existing import, search, SQLite-rollback, ownership, UI geometry and native integration checks are mandatory on both existing Windows runners before release. An unchanged source-file batch remains a no-op in the database even though its visible selection changes.

This validates screen-filter correctness on test fixtures. It does not measure actual Windows 11 input-to-render latency, human task-time savings, offline network-drive performance, or multiday adaptation; those remain in issue #22.

