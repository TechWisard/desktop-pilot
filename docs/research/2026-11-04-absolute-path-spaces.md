# Copied absolute paths with spaces must select one launch profile — 2026-11-04

## Why this matters

The catalog supports searching by a shortcut's source or target path. An absolute path copied from Windows File Explorer may contain folder names with spaces, e.g. `"C:\Users\Editor\Team Alpha\Shortcuts\editor.lnk"`. When several launch profiles share an executable or similar paths, finding the wrong entry risks applying a manual **Never** or **Always** rule to the wrong profile.

Earlier research: [copied-path search](2026-11-04-copied-path-search.md), [catalog relevance](2026-10-28-catalog-search-relevance.md), and [stable manual choices](2026-10-30-manual-placement-explanation.md). Primary contextual references: [Microsoft File Explorer](https://support.microsoft.com/en-us/windows/experience/fileexplorer/file-explorer-in-windows), [Findlater & McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) (user control in adaptable menus). These references establish the workflow and design motivation, **not** that people encountered this particular defect in Desktop Pilot.

## Source-confirmed defect and pre-change reproduction

The existing `CatalogCache::update` normalizes path separators but then splits *every* query on whitespace and accepts each token if it appears **anywhere** in the concatenated searchable fields. For a copied path containing `Team Alpha`, this admits a second profile whose source has `C:/Users/Editor/Team Beta/...` and whose note contains `Alpha/Shortcuts/editor.lnk`. The full pasted path never occurs in that second entry. It nevertheless passes the old two-token predicate. This is a concrete source-derived failure case, not telemetry from a real user.

## Fix and production-path verification

If a query, after trimming surrounding whitespace and double quotes and normalizing slashes, begins with a drive-root form (`C:/`) or UNC root (`//server/`), treat it as **one contiguous search substring**. Preserve the existing token search for names, loose keywords, and non-absolute searches. The same cached catalog, existing SQLite entries, explicit filters and sorting are used; no independent recommendation model is introduced.

The regression in `src/ui/catalog_relevance_tests.rs` creates three distinct profiles in actual `Store`/SQLite and exercises the production `CatalogCache::rebuild` and `update`: drive paths with spaces and quotes, forward and backslashes, unquoted absolute paths, executable targets, UNC network paths, manual Never filter, a missing source, the intentionally broad keyword fallback, and persisted source integrity. It requires the correct single result rather than the former correct-plus-decoy list.

**Expected before/after in the synthetic fixture:** absolute path lookup: 2 matches under the old token predicate → 1 correct profile with contiguous lookup. No file moves, identity changes, policy changes, or network services are involved. This is not a measured improvement in real-world task duration.

## Remaining acceptance

Verify clipboard paste in the actual Windows 11 UI at 100–200% DPI, real OneDrive and UNC locations, complex filenames, partial path searches and user-visible rules. Check normal Windows Rust/native CI and do not interpret this single scenario as broad usability validation. Preserve the draft organizer's separate acceptance boundary.
