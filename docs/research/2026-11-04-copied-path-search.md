# Quoted Explorer paths must locate existing shortcuts — 2026-11-04

## User task and evidence

A person managing duplicate shortcuts or multiple launch profiles may know the original file's location but not its catalog display name. Windows File Explorer supports copying the full selected path (Ctrl+Shift+C). Such copied paths commonly include enclosing double quotes. Windows shortcut sources contain backslashes, while a pasted path from a browser, script or other tool may instead use forward slashes.

- [Microsoft support: File Explorer in Windows](https://support.microsoft.com/en-us/windows/experience/fileexplorer/file-explorer-in-windows) — primary reference for the Explorer workflow.
- [Windows community report, May 2026](https://www.reddit.com/r/Windows11/comments/1t9es16/tip_of_the_week_if_you_have_a_file_or_folder/) — users specifically discuss copying quoted paths. This is evidence of the workflow, **not** a measured Desktop Pilot defect.
- [Existing catalog relevance scenarios](2026-10-28-catalog-search-relevance.md) — exact names must beat path-only hits when the search is broad; large catalogs must remain complete.
- [Existing import-filter scenarios](2026-10-28-last-import-filter-state.md) — editing a search must not erase manual choices or the last-import view.

## Source-confirmed gap

In v0.2.113, `CatalogCache::update` searches lowercased words as literal substrings of `search_text`. A copied query such as `"C:\\Users\\Editor\\Work\\Shortcuts\\exact.lnk"` does not match the existing source `C:\\Users\\Editor\\Work\\Shortcuts\\exact.lnk`, because the leading/trailing double quotes are not present in the stored source. A path typed with `/` does not match the same source stored with `\\`. The program can therefore show **zero results for an existing shortcut**, making manual rule changes and profile inspection unnecessarily difficult.

This is a source-level reproduction, not an observation from a measured user session.

## Correction

Normalize `\\` and `/` to the same separator **only for the searchable catalog text and user query**; trim surrounding double quotes from query tokens. Do not modify `App`, stored `source`/`target`, `App::identity`, SQLite, launch arguments, ranking, user choices or shortcut files. Preserve tokenized multiword matching, explicit filters, relevance sorting and pagination.

The new regression in `src/ui/catalog_relevance_tests.rs` uses the existing real `Store`/SQLite catalog fixture and `CatalogCache::update` path. It verifies quoted Windows paths, unquoted paths, forward-slash paths, a target executable path, whitespace around copied paths, the explicit Never filter, negative lookup, and multiword relevance. No separate mock recommendation engine is introduced.

## Comparison and limits

Before: a stored source with `\\` could be invisible to a copied quoted query or slash-equivalent query.

After (expected): the same catalog entry is found without creating duplicates or changing stored identities; original exact-name relevance and rule filters remain intact.

The scenario is synthetic. It does not establish measured time savings or a production crash. Full Windows Rust tests, Clippy, format check, release guards, and UI acceptance must be reported from actual CI; real Windows 11 interaction at 100–200% scaling remains a separate release gate for the draft organizer feature.

## Follow-up

Check the working client with paths containing spaces, network shares, multiple launch profiles, and non-ASCII names. For long-running adaptation work, prioritize stable manual placement, interrupted file operations and safe undo rather than introducing new search controls.
