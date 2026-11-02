# Direct desktop shortcut discovery under a shared scan cap — 2026-11-02

## Source evidence and user scenario

Continue the existing [bounded discovery study](2026-11-02-stable-bounded-discovery.md) and [adaptive shortcut research](2026-10-26-adaptive-shortcuts.md). Windows [FindNextFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findnextfilew) does not guarantee the order of directory enumeration. The [CHI 2004 controlled comparison](https://doi.org/10.1145/985692.985704) found that a user-adaptable menu was preferred to an automatically rearranging one by most of 27 participants. These sources motivate predictable access and respect for choices, but neither proves that users will save time with this change.

**Confirmed source-path problem:** the production visitor sorted each directory and then descended immediately into alphabetically early directories. A large nested project folder could consume the entire global 6,000-file budget before a directly visible, alphabetically later desktop shortcut was considered. The later global sort happened *after* the budget was consumed and could not recover that missing source. This is a constructed but realistic bounded-scanning case, not an observed failure rate.

## Correction

Within each directory, consider direct `.lnk` and `.url` shortcuts first, in stable name order, then recursively visit accepted child folders in the same stable order. Continue using the **same** total file cap, depth limit, exclusions, metadata/reparse checks, first-root precedence and warning. Existing catalog entries and manual Always/Never choices are not deleted; no user files or placement geometry are touched. No additional setting, telemetry, background service or external dependency.

This favors the explicit visible shortcut over an arbitrary deep source when the catalog is saturated. It does not promise full coverage of every nested directory or a particular search latency on physical disks.

## Production-path regression and before/after

`direct_desktop_shortcuts_win_the_shared_budget_independent_of_creation_order` uses real temporary `.url` files and the actual `visit_shortcut_paths` production function with two directory trees created in reverse order. Five paths: `z-root.url` directly on the desktop, `01-nested/{a,b,c}.url`, and `09-other/d.url`.

- At budget **2**, before: nested `a,b`, **0** direct desktop shortcuts. After: `z-root` and nested `a`, **1** direct shortcut. Both scans remain within the cap.
- At budget **4**, before: nested `a,b,c` and other `d`, **0** direct shortcuts. After: `z-root` plus nested `a,b,c`, **1** direct shortcut.
- Both creation orders must return identical results. The existing 5,000-link test continues exercising full SQLite discovery, manual policy persistence and original-byte preservation. The regression tests deterministic source inclusion, not real-world task completion time.

## Remaining acceptance

Run standard strict Windows Server 2022 and 2025 CI: rustfmt, all Rust tests, Clippy with warnings denied, build, native integration checks, release guards. Check that no temporary success artifacts are retained. Live Windows 11 Explorer, very slow directory trees, redirected desktops, desktop DPI/visual behavior, and users' long-term recommendation quality remain unverified. Keep organization drafts #98 and #101 separate.
