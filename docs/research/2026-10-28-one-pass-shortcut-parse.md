# One verified decode per saved shortcut during catalog scans (2026-10-28)

## Starting point and independent context

Baseline: published Desktop Pilot 0.2.36, main `4c7ed12c99f54bb189d147dfeb54c2512a7e3048`; main CI [40696427393](https://github.com/TechWisard/desktop-pilot/actions/runs/40696427393) passed. No open PR was found. This continues [cold-start acceptance issue #22](https://github.com/TechWisard/desktop-pilot/issues/22), not a new Windows compatibility matrix. Previous changes to 5,000-entry discovery, cached search, stable placement, low-write rescans and the nonblocking scan worker remain intact.

Primary HCI work on predictable adaptive interfaces remains [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) and [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml); neither measured Desktop Pilot's scanner. An independent [Flow Launcher custom-source issue #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), reported May 21, 2026 and closed June 8 after plugin updates appeared to resolve it, motivates large-catalog tests; it does not establish the same bug or measured delay here.

## Confirmed inefficiency

In `engine::refresh_apps_once`, each saved app is already passed to `native::read_link` during the verification pass. A second pass enumerates the desktop and Start-menu shortcut paths, calls `native::read_link` again for that exact same verified original, then discards the repeated result because the saved original remained valid. The second decode provides no additional state in the stable-source case. This doubles real Windows shortcut/URL parser invocations for a warm catalog, before any actual new/changed shortcut has been found.

A test-only counter in the exact production call wrapper `catalog_link` records actual decoder invocations without running third-party programs, changing the parser or introducing a separate simulated catalog. The baseline branch [red run 40698863291](https://github.com/TechWisard/desktop-pilot/actions/runs/40698863291), using unchanged 0.2.36 scan logic plus the new assertion, failed on **both Windows Server 2022 and 2025**: **200 decoder invocations for 100 saved originals** instead of the required one per original; 266 other Rust tests passed per runner. The earlier run [40698435434](https://github.com/TechWisard/desktop-pilot/actions/runs/40698435434) was cancelled by a newer push and is not used as behavioral evidence.

## Narrow change and acceptance

After an original was successfully parsed with the **same full launch identity** and marked valid on the current scan, remember its normalized source path in a temporary in-memory set. When discovery encounters **that same path**, do not parse it a second time. Other files still use the full original parser and identity index. A changed target, unavailable original, unreadable/partially written file, independent alias or newly added shortcut does not enter this verified set and continues through the existing discovery/recovery path. Ownership snapshots and the pre-commit conflict guard remain in place; original files are never rewritten.

`src/engine/identity_index_tests.rs` now requires exactly one decode per stable saved original on isolated folders of **100 and 1,000 real Windows .url shortcuts**, while checking Always/Never choices, usage, preview without unwanted additions/removals, persistent desktop position, original bytes and real Undo. Existing alias recovery, edited-target, manual move and concurrent scan regressions remain mandatory. The new counter exists only in test builds; there is no new setting, schema, dependency, background service or telemetry.

The initial corrected [branch CI 40698773663](https://github.com/TechWisard/desktop-pilot/actions/runs/40698773663) passed both Windows runners with 267 Rust tests, 28 native checks, nine release guards, formatting, warnings-denied Clippy and a locked build. The final stricter exact-count assertion and version metadata require normal PR/main checks independently before publication.

## Interpretation and continuation

This fixture demonstrates a reduction from **200 to 100 decoding calls for 100 unchanged sources**, and the strict corrected fixtures require **100 and 1,000**, respectively. It does **not** demonstrate a universal 50% reduction in filesystem I/O or in startup latency: directory enumeration, path metadata, SQLite operations and other app/alias parsing still occur. No physical Windows 11 task timing, network latency, power use or human productivity gain was measured.

Continue issue #22 with measured time to first saved search and preview on physical Windows 11 for 100/1,000/5,000 entries at 100% and 200% DPI; real disconnected network/cloud roots, inaccessible profile drives, and multiweek Work/Personal/rare-app and weekend transitions. Record false placements, redundant changes, adaptation lag, explanations, user steps, safety and Undo. The unchanged protective CI must pass before release.
