# Catalog search relevance without disrupting user-defined order (2026-10-28)

## Starting state

The verified baseline was Desktop Pilot 0.2.34 at `9a2f867e9876ba3f29b27f4d80c3f9439886fd24`; no open PRs were found. Earlier work on no-op SQLite rescans, cached catalog search, manual placement, source continuity, worker cancellation, and launch profiles remains unchanged. This investigation targets time to **find** a known app, not another Windows-version matrix.

## Independent evidence, separated from observed behavior

- Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces*, CHI 2008 (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml): improving accuracy and predictability increased satisfaction; higher accuracy also improved measured performance and adoption. This is motivation, not a Desktop Pilot experiment.
- PowerToys Command Palette issue [#46269](https://github.com/microsoft/PowerToys/issues/46269), opened 2026-03-19: a reporter found an exact filename match near the bottom of search results. The issue is marked resolved/helped, concerns another product, and proves neither Desktop Pilot prevalence nor common root cause.
- The production `CatalogCache::update` in Desktop Pilot 0.2.34 performs a complete metadata match but preserves alphabetic catalog order when the user chooses the default sort. If the word appears in a common parent directory, exact-name hits can appear after thousands of irrelevant names. This is directly verifiable in our source, independent of the reports above.

## Production-path reproduction

`src/ui/catalog_relevance_tests.rs` saves 100, 1,000 and 5,000 shortcut records to the **real SQLite Store**, then executes the actual production `rank`, `CatalogCache::rebuild` and `CatalogCache::update`. Every entry is associated with a common `C:\\Users\\Editor\\Work\\Shortcuts` path. The query "Editor" matches every row, but only one app is named exactly "Editor". Against baseline ordering, the exact app appears after the 100/1,000/5,000 alphabetically earlier "A..." entries, so a user can fail to find it on the first result page. No real applications or files are launched.

The tests also cover multiple words, query case and whitespace, name prefix/phrase matches, unchanged result completeness and unique indices, manual Never filter, an explicit usage-time sort, and the empty-query alphabetic ordering. These are deterministic synthetic correctness and search-action proxies, not human timings or network-share measurements.

## Narrow correction

When a **non-empty query** is used and the selected order is the default, give priority to exact app title, title prefix, title suffix, contained phrase, all query words in title, some title words, then metadata-only matches. Within each priority retain the previous alphabetic order. The existing search predicate is unchanged, so every result remains available. The dropdown identifies the contextual default as **By relevance**, while an explicit **Alphabetically** option remains available during search. User-selected ranking, usage-time and alphabetic sorts are not overridden. Manual policies, recommendation scores, desktop positions, original shortcuts, history, Undo and every SQLite schema remain unchanged. No new panels, telemetry, service or dependency.

The production search uses `sort_by_cached_key` so each matching title is lowercased once for relevance ordering rather than once per comparison. Empty queries do no extra sorting. The new tests use the real saved-catalog path; they cannot establish measured latency or time savings on a physical Windows 11 desktop.

## Verification and continuation

Compare new tests at the baseline commit against this patch (failure-first on immutable test branch), then execute the repository's existing Windows 2022/2025 checks unchanged: formatting, locked Rust suite, warnings-denied Clippy, build, 28 native integration fixtures and nine publication guards. Merge and publish only after those pass; verify tag, main, source, executable and public checksums separately. Keep issue #22 open for cold-start 100/1,000/5,000 **wall-clock** timings, physical Windows 11 at 100% and 200% DPI, network disconnects and multi-week habit adaptation. Those outcomes are not established by this UI-search fix.
