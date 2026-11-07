# Exact lookup for copied Windows batch targets — 2026-11-07

## Why this matters

A shortcut catalog can contain several same-named launch profiles and explicit **Always**/**Never** choices. When someone pastes the full path of a launcher from Explorer, the selected policy must belong to that exact target, not to another catalog entry whose note mentions the path. This is especially important for less frequently used maintenance and project launchers.

Microsoft documents `.bat` and `.cmd` as distinct batch-program filename extensions: [Windows `call` command](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/call). [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) found a controlled 27-person adaptive/adaptable-menu comparison generally favored direct user control over shifting adaptive menus. These references motivate predictable lookup but **do not measure Desktop Pilot users**, nor establish how often batch launchers are used.

## Source-confirmed inconsistency, not a field incident

`CatalogCache::update` already treats a complete pasted absolute `.lnk`, `.url` or `.exe` path as exact identity. A complete `.cmd` or `.bat` path instead falls into general substring matching of names, paths, arguments, categories and notes. A user pasting `…\\Team Alpha\\Launch.cmd` can therefore see a shortcut with that true launch target **and** an unrelated shortcut mentioning the path in its note, potentially under different manual-policy filters. A name with a longer suffix may also match. This is deduced from source and reproduced in a synthetic fixture, not an observed real-user complaint.

## Test plan and production-path checks

The regression `copied_batch_launch_paths_identify_only_the_saved_target_and_policy` uses actual local `.cmd` and `.bat` files, distinct lookalike target names, policy-bearing decoys, a real SQLite `Store`, the production catalog ranking and `CatalogCache::update`. It checks quoted full paths, slash variants, Never/Always filtering, broad word search, and unchanged bytes and persisted catalog. It does not execute the scripts.

The minimal correction is to include `.cmd` and `.bat` in the existing exact-filename path branch. Source path and target normalization, manual choices, preview, placement, history and database schema stay unchanged. A partial folder/name search remains intentionally broad. This reduces misleading matches and actions needed to inspect the right saved policy **in this controlled case**, not demonstrated elapsed time saved in field use.

## Acceptance and remaining work

Run normal public Windows Server 2022 and 2025 CI: `cargo fmt --check`, locked Rust tests, strict Clippy, build, native integration and publication guards. Capture failing-baseline and passing-after checks on the real search code. A green CI result does not prove physical Windows 11 Explorer input, human error rates, performance or UI scaling. Keep the separate organizer PRs #98 and #211 under their own live acceptance requirements. Follow up with mixed short/long project-launch sequences, unusual extensions and Windows 11 interactive searches without adding background telemetry.
