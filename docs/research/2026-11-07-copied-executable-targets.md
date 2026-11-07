# Copied executable targets with literal percent-named folders — 2026-11-07

## Why this matters

The Apps search has two different uses: a broad lookup by words or folder, and an exact lookup when someone pastes a full path from Explorer. A person managing two launch profiles in different folders must be able to inspect the correct saved **Always** or **Never** rule before changing it. This is especially important when shortcuts share a display name. An exact path is a location, not a command template.

This follows the earlier [shortcut source-path regression](2026-11-06-literal-copied-path-search.md); its source/URL fixes remain unchanged. No new general search was necessary. Microsoft's [file-name documentation](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) distinguishes concrete file paths from shell command expansion. [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) studied the predictability cost of adaptive versus user-controlled menus; [Amershi et al., CHI 2019](https://doi.org/10.1145/3290605.3300233) recommend making adaptation understandable and corrections easy. Neither source evaluates Desktop Pilot.

## Source-confirmed defect

In `CatalogCache::update`, copying an absolute `.lnk` or `.url` already uses `norm_literal_path` to compare the saved source. Copying a full `.exe` path, however, still compared the saved launch target with `norm(&app.target)`, which expands `%VARNAME%` sequences **inside concrete absolute paths**. Windows permits a real folder named `%USERNAME%` beside the folder named after the user.

The following saved records can therefore be confused:
- `...\%USERNAME%\Rare Tool.exe` with a **Never** rule;
- `...\runneradmin\Rare Tool.exe` with an **Always** rule.

The exact path of the first used to match **nothing**; the path of the second could match both records. An unrelated note mentioning the first path is not evidence of the first target. These findings describe the application source path and a deterministic fixture, **not an observed user incident**.

## Failure-first reproduction and correction

The [test-only Windows Server 2022 run](https://github.com/TechWisard/desktop-pilot/actions/runs/42341478585) compiled the actual product and ran `CatalogCache::update` on real, distinct Windows filesystem names and a persisted SQLite catalog. The new regression failed for the quoted literal executable path (returned `[]` instead of `[literal]`); **427 other Rust tests passed** on that runner. This independently confirms the original search failure without a mock search function.

The corrected exact-file branch now normalizes an already absolute executable target **literally**, while retaining expansion for a relative saved launch template such as `%SystemRoot%\System32\notepad.exe`. The production search and its filters are exercised with two policies, quoted and slash-separated full paths, a competing note, a template target, and unchanged fixture bytes and persisted metadata.

Expected acceptance: one result for the selected literal path, one result for the expanded sibling, no wrong-policy results, template lookup unchanged. The test is a controlled technical scenario, not a measured improvement in user task times. The change does not launch the placeholder executables, alter the saved database, write to the desktop or change automatic placement.

## Validation boundary and follow-up

Run `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, native integration and publication guards on the existing public Windows runners. This corrects a search identity boundary, not the broader launch-identity migration contract. Explicitly test unusual **absolute user-authored launch templates** containing percent variables against real offline files before changing any persistent identity rule. Keep the separate draft desktop organizer and its live acceptance gates unchanged.
