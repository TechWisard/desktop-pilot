# Quoted and variable-root shortcut paths in catalog search and import — 2026-11-05

## Observed behavior versus hypotheses

**Source-confirmed defect:** `src/ui.rs` recognized only double-quoted imported paths. A legitimate Windows absolute shortcut path pasted with PowerShell-style single quotes was retained with literal apostrophes, then rejected as not absolute by `validate_shortcut_path`. Search also treated the pasted single-quoted full path as free-text words rather than as the exact saved shortcut, potentially missing an important saved application. A root such as `%SystemRoot%\...` was not considered absolute for importing even when `model::expand_env` could resolve it. These are reproducible source behaviors, **not** independently reported Desktop Pilot incidents.

**Independent evidence:** [Microsoft PowerShell quoting rules](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_quoting_rules) document both single and double quotes. [Microsoft cmd variable substitution](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd#substituting-environment-variable-values) documents `%VariableName%`. [Microsoft Win32 environment expansion](https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-expandenvironmentstringsw) documents `%variableName%` behavior. Prior local source and tests already cover copied Explorer double quotes, exact path identity, extended drive and UNC aliases, manual Never and unrelated backup profiles: `2026-11-04-copied-path-search.md`, `2026-11-05-copied-file-path-identity.md`, `2026-11-05-extended-copied-path-search.md`.

## Narrow correction

Use one matching-outer-quote helper for Search and pasted Import. Resolve environment-rooted paths before detecting complete absolute path queries. Preserve exact-source matching for `.lnk` and `.url` and exact-target matching for `.exe`; never promote a note, an incidental matching directory, or a backup to the chosen shortcut. During Import expand variables **only if** the entered path is not already absolute and expansion yields an absolute path; otherwise preserve the literal path and existing validation. Full paths with a real `%NAME%` component are not silently rewritten. No launch, policy, SQLite schema, background hooks, private file data or telemetry changes.

## Reproduction and checks

`powershell_quoted_and_environment_root_paths_find_only_the_saved_original` uses the production `Store::open`/SQLite catalog, `CatalogCache::update`, `shortcut_paths`, `validate_shortcut_path`, `model::norm` and ranking with a synthetic rare shortcut and a decoy note/backup. It checks single and double quotes, a variable root, deduplication, manual Never versus Always filters, preservation of stored app records, continued free-text search and refusal of relative imported names. Existing copied-path relevance and profile tests remain enabled.

**Expected before:** single-quoted queries return no exact match; a variable-root import remains relative. **Expected after:** one exact saved app is selected; import parser returns one canonical path without moving any file. Synthetic matches and test counts are not proof of time saved by actual users.

## Remaining acceptance

Run the unchanged strict Windows 2022/2025 CI (Rust tests, fmt, strict Clippy, build, native integration, publication guards). Confirm a real Windows 11 copy/paste flow and RU/EN UI at 100–200% DPI before calling this behavior physically validated. Do not use this mainline improvement as a substitute for the draft desktop organizer's separate Undo/OneDrive acceptance.
