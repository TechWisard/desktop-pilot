# Exact lookup for less common Windows command targets — 2026-11-07

## User scenario and evidence

A person may keep distinct launch profiles for rarely used administrative consoles, diagnostic programs, and scripts, with different **Always** or **Never** rules. Pasting the full path to such a target in Apps should find the saved profile, not a different shortcut whose note contains the same path or whose target has a longer suffix.

Microsoft's [`start` command reference](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/start) lists default Windows PATHEXT entries `.COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC`. The earlier [exact batch-target study](2026-11-07-copied-batch-targets.md) documented and reproduced the same search error for `.cmd` and `.bat`. The present extension list is grounded in Microsoft's primary documentation, not an invented list of supported Desktop Pilot installation formats.

Findlater and McGrenere's [CHI 2004 adaptive/adaptable menu study](https://doi.org/10.1145/985692.985704) supports investigating stable, directly controllable selections. It does **not** measure use of these launchers or any Desktop Pilot productivity benefit.

## Source-confirmed gap

At stable commit `53edfde`, `CatalogCache::update` treats pasted absolute `.lnk`, `.url`, `.exe`, `.cmd` and `.bat` as exact source/target identities. Other default Windows command-target extensions fall through to broad metadata substring matching. A saved `.msc` or `.com` target can therefore be confused with a note referencing the same path or with a backup target. Manual policies may differ.

The correction applies the existing exact-path behavior to the remaining PATHEXT extensions. It does not change source detection, shortcut creation, launching, ranking, file mutation, configuration, placement, history or database schema. Relative queries, ordinary text search, and manually entered environment-variable templates keep their existing behavior.

## Regression and acceptance

`copied_windows_command_paths_keep_saved_profiles_and_manual_rules_distinct` creates local synthetic files for the eight newly covered suffixes, two distractors per target, and different persisted manual policies. It runs the real SQLite Store, `rank`, and `CatalogCache::update`, verifies quoted/slash paths, Always/Never filters, broad word search, untouched bytes and catalog contents. It **does not execute** the scripts or establish their launchability on arbitrary machines.

Required checks on the normal public Windows pipeline: format, locked Rust tests, strict Clippy, locked build, native integration and release guards. This is a production-path **synthetic scenario**, not a field usability study. Real Windows 11 interactive paste behavior, physical multi-monitor acceptance and the separate unreleased organizer remain independent outstanding work.
