# Preserve manual placement during an overlapping shortcut scan — 2026-10-28

## Baseline and user benefit

Starting point: public Desktop Pilot 0.2.33, main `6b10bfd5d1a0ab93c0b8bbd835c7c60bc772c515`; its Windows CI and public release verification had passed, and there were no open pull requests at investigation start. Existing work on catalog snapshots, large-library scanning, manually renamed quick copies and simultaneous imports is retained. This investigation extends the real-world scenarios in [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22), without repeating the full earlier research.

Primary HCI evidence: Findlater and McGrenere, [CHI 2004](https://doi.org/10.1145/985692.985704), compared static, automatically adaptive and user-adaptable menus; participants often preferred manual adaptation, and adaptive rearrangement was not consistently faster. Gajos et al., [CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), found that better predictability and accuracy improved interface satisfaction, with accuracy benefiting performance and use. These studies support prioritizing predictable manual control, not a quantitative claim about Desktop Pilot. Independent [Flow Launcher issue #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), reported 2026-05-21, illustrates delayed custom-catalog availability in another tool; its reporter later described a likely plugin update fix. It is not evidence of an identical bug in Desktop Pilot.

## Reproduced production defect

A background `refresh_apps` reads catalog records and `State.owned` before scanning files. During its file operations, a person may explicitly apply a permanent quick shortcut; this changes `State.owned` and creates a new file in the managed desktop zone. If the original shortcut then becomes temporarily unavailable, the still-running scan uses its *old* ownership snapshot and can mistake the newly managed copy for an independent original. A user rename before the scan enumerates the desktop has the same failure. The scanner could then replace the saved source path and undermine subsequent Undo, even though manual Always and original file identity should be authoritative.

The already-existing source-import race guard checks newer `App.source` values, but does not detect changed `State.owned`. This is a source-confirmed scheduling gap, not merely an inferred Windows-version compatibility issue.

## Correction and safety boundary

A scan now compares the freshly loaded owned-shortcut state to the state seen before filesystem I/O **inside its actual SQLite write transaction**. If different, the entire stale result is discarded *without any catalog write* and one new production scan is attempted with current ownership. This preserves the existing file-identity protection for renamed managed copies. If ownership changes again, the scan reports an actionable error rather than committing guesses or performing unbounded rescans. The exclusive writer transaction prevents a new placement commit between the final ownership comparison and scan writes.

No ranking threshold, user preference, source file, manual position, telemetry, dependency or database schema is changed. Ordinary byte-identical aliases with independent filesystem identities must remain eligible as originals. Undo remains conservative: a quick copy **renamed by the user** stays on disk after Undo releases its ownership; the application must never silently erase that file.

## Red/green reproduction on real logic

Test-only `BEFORE_REFRESH_DISCOVERY` injects an explicit real `apply(store, true)` after the scanner has taken its ownership snapshot but before file reads. The tests use an isolated real SQLite `Store`, native Windows `.url` parsing, actual desktop-copy creation, source outage with `std::fs::rename`, `refresh_apps`, `reconcile_manual`, `preview`, and `rollback`. No separate ranking/scan model is substituted.

1. Create a previously unknown managed copy during the scan, remove the original, and assert the catalog source, Always, note, original bytes, copy and Undo remain safe.
2. Rename the newly managed copy before discovery, then remove the original. Assert its file identity is not adopted as a new source; preserve manual placement and user-renamed file on Undo.
3. Create a byte-identical **independent** desktop alias alongside the managed copy, remove the original, and confirm the independent alias can still be selected without stealing managed ownership.
4. Check the new recovery message in both languages and retain all prior source-import, offline, edited-copy, large-catalog and Windows native regression fixtures.

Failure-first [run 40658727599](https://github.com/TechWisard/desktop-pilot/actions/runs/40658727599) ran only test additions on the **unchanged 0.2.33 implementation**: 259 tests passed, two new ownership assertions failed, and the independent-alias control passed on both Windows Server 2022 and 2025. These were behavioral source-path failures, not formatting or compilation failures.

After the bounded ownership retry and correction to a test's unsafe assumption that Undo should delete a user-renamed file, [run 40659360500](https://github.com/TechWisard/desktop-pilot/actions/runs/40659360500) passed unchanged Windows CI on both runners: **262 Rust tests, 28 native checks and 9 publication guards** per runner, together with bilingual README validation, `cargo fmt --check`, warnings-denied Clippy and `cargo build --locked`. This was the checked implementation before the release-version-only commit; final PR/main CI and published files require their own verification.

## Quantitative boundaries and remaining work

In these three controlled interleavings, the previous code made two incorrect source adoptions; the corrected code made zero and preserved the independent-alias behavior. This is a deterministic correctness comparison, **not** a measure of real user time saved or prevalence. The test does not reproduce a physical Windows 11 Explorer session, a real cloud-sync outage, a 200% DPI screenshot, a 5,000-shortcut timing benchmark or multiweek habit changes. Continue issue #22 for those investigations, with false-placement, unnecessary-movement, latency, reasons and Undo metrics. Keep this release limited to the verified ownership fix.
