# Copied literal Windows shortcut paths in Apps search — 2026-11-06

## Confirmed product behavior

This follows [the import-path correction](2026-11-06-literal-percent-shortcut-import.md) and [filesystem-boundary review](2026-11-06-literal-filesystem-boundaries.md). No repeated general search is needed. The Settings **Apps** catalog still searched saved `.lnk` and `.url` sources using `model::norm`, which expands percent-delimited environment tokens. Unlike an entered environment template, an absolute path copied from Explorer is already a materialized filename. A real directory called `%USERNAME%` can coexist with the directory named after the current Windows user. When each contains `Rare Tool.url`, searching for the copied path of either one could select **both** or the wrong one, hiding the distinction between their saved Always/Never settings.

This is a source-confirmed defect in search identity, not a user incident or measurement of real usage. Microsoft documents the distinction between ordinary file names and command-line environment substitution: [Naming Files, Paths, and Namespaces](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) and [cmd variable substitution](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd). [Amershi et al., CHI 2019](https://doi.org/10.1145/3290605.3300233) emphasize efficient correction, predictable adaptation and visibility of system behavior; this paper does not measure Desktop Pilot.

## Practical scenario and correction

A person keeps two same-named shortcuts for separate projects and makes one **Never** and the other **Always**. They paste the first shortcut's full Explorer path into Apps to review its rule. Previously the search used `norm` for both entered path and saved source, causing different literal filesystem paths to have the same identity. That is particularly harmful when changing the wrong rule while the visible shortcut names match.

The Apps catalog now:
- normalizes pasted **absolute** Windows paths literally, including quoted and slash-converted variants, without environment expansion;
- continues intentionally expanding relative `%SystemRoot%\...` templates into absolute paths at the input boundary;
- normalizes the already saved shortcut **source** literally, while keeping existing executable launch-target template expansion and URI matching behavior.

No source shortcuts, user preferences, taskbar pins, history, database format, ranking rules or background collection are altered. The change is to search and display identity only.

## Reproduction and acceptance

`src/ui/catalog_relevance_tests.rs::copied_literal_percent_directory_finds_only_its_own_saved_shortcut` constructs **real Windows folders** with literal `%USERNAME%` and expanded username spelling, writes independent valid `.url` shortcuts with the same leaf name, reads them through `native::read_link`, stores different Always/Never rules via the actual SQLite `Store`, and queries the actual `CatalogCache::update` used by Settings.

It checks quoted Explorer paths, PowerShell-style quotes, slash variants, both policy filters, notes containing a competing path, unsuccessful exact matches, unchanged persisted app records and unchanged original shortcut bytes. Existing tests cover search relevance, extended UNC/drive forms, explicit `%SystemRoot%` template search, case-sensitive web destinations, sorting and large catalogs.

**Synthetic before/after criterion:** the old normalization maps these two concrete filenames to one search identity (potentially two results for one copied path); the corrected search must return the single intended saved ID, without affecting file bytes or rules. This is a constructed boundary case, **not** proof of minutes saved by users.

## Release and evidence boundaries

Run the existing public Windows 2022/2025 checks without removing any gates: `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, native fixtures and release guards. Preserve the separate organizer PR's unverified Windows 11 GUI, OneDrive, interrupted-operation and live-desktop acceptance gates. Physical Windows 11 keyboard/screen-reader and 100–200% DPI testing remain necessary; do not mistake this synthetic fixture for field validation.