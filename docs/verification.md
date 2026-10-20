# Verification scope — 0.2.3

[Русский](verification.ru.md) · [Home](../README.md)

## Release gates

The Windows workflow checks formatting, runs the test suite, rejects Clippy warnings, builds the executable and runs `scripts/test-native.ps1` on both `windows-2022` and `windows-2025`. Publishing depends on both jobs succeeding. The optimized release executable runs the native fixture again before packaging. The package script checks its Windows product version against Cargo metadata and produces SHA-256 checksums for both archives. The release links the C runtime statically to avoid a separate Visual C++ Redistributable requirement.

Nine isolated cases exercise the publication script: a new release, draft continuation, an annotated tag, a mismatched tag, an already published release, an incorrect checksum, a missing asset and a main-branch change before or during upload. Publication verifies the commit, file set, sizes and SHA-256 digests through the GitHub API. A mismatch leaves the release unpublished.

[Actual workflow results](https://github.com/TechWisard/desktop-pilot/actions/workflows/checks.yml) are the authoritative record for a given commit. The release archives are built from the published tag's commit.

## Regression coverage

- Shortcut identity, exact `.lnk` / `.url` copies, case-sensitive URL paths and arguments, Windows names and duplicate display names.
- Stable ownership through rename, deletion and manual edits; byte-identical user files cannot be silently adopted or overwritten.
- Prepared and partially completed operations, interrupted undo, staging collisions and recovery after process interruption.
- Atomic configuration and catalog writes, nested transaction rollback, old-profile migration, unrelated database refusal, corrupt-data reporting and command recovery.
- Ranking stability, manual precedence, independent active days, restart continuity, measured elapsed time, sleep gaps and learning thresholds.
- Foreground interaction evidence, stale/injected input rejection and no ranking increase from a launch request alone.
- Profile selection, quoting, installation and replacement, preservation of profile data, and refusal to overwrite an unrelated executable.
- UTF-8/BOM, UTF-16 and INI parsing; taskbar argument matching, Steam path bounds and Windows resource handling.
- Settings merging, conflict handling, saved versus draft state, operating modes, search, interface scale and localization that preserves filenames and paths.
- Consecutive undo of replacements at the same shortcut path, including rejection of a byte-identical user file; atomic new-database initialization and concurrent first opens.
- Cancelling old queued commands on restart, long initial learning with alternating brief and qualified days, and unchanged ranking results after optimizing large-catalog processing.
- Preserving complete disable through settings preparation and updates, restoring background work after a failed executable replacement, refusing installation over a newer version, and retrying an unavailable tray.
- Atomic report writes, retaining an existing report when replacement fails, and protecting the database even through its Windows 8.3 short name.
- Correct undo order after a backward clock correction; retaining a working copy when the original is temporarily unreadable, including a real file-sharing denial.
- No extra sessions after a brief switch following an hour of use or after a restart; keeping shutdown signalled until the process releases its instance.
- Waiting for the previous worker to stop and the new worker to become ready; resolving paths and positions from the same Shell identifier, including a virtual desktop item.
- Nonfatal Start menu shortcut failure, retry and preservation of unrelated files; no diagnostic writes in rejected folders and safe replacement of a hard-linked error log.
- Shared keyboard/selector zoom persisted across reopening; search and pagination with 3,000 apps, rule/ranking refresh, and successful/failed rule confirmation.

The geometry matrix covers **5 resolutions × 5 grid/scaling configurations × 4 desktop origins = 100 combinations**, including occupied and undersized areas. Existing positions are preserved by the placement calculations.

## Native fixture and interface checks

The native fixture uses new isolated `test-*` profiles. It checks 28 end-to-end conditions: shortcut creation and identity, a real Shell launch of an inert test executable, operation apply/undo/recovery, original byte preservation, manual rename/delete/edit handling, refusal to overwrite modified files, clean installation, background startup/shutdown, observation-only behavior and the initial learning threshold. Additional cases cover cancelling stale commands and completing a fresh command, complete disable and explicit resume, preserving disable when preparing Settings, database protection during report export, catalog diagnostics without creating a profile, restoring background work when an open file handle actually prevents executable replacement, replacing an already stopping worker, and three cases preserving files during error reporting. It does not launch user applications or modify the user's desktop/startup entries.

Local checks also run the Windows executable under Wine. The 0.2.3 interface matrix contains **24 scenarios and 123 screenshots**: Russian and English Home, Apps, all five settings pages and History at 1120×820, small 720×540 windows at 150–200% interface scale, and 3840×2160 captures at 125%, 150% and 200%. The three setup steps and long pages are checked from both the top and bottom at 200%. Separate captures cover app details, a pending rule, search and the final page of a 3,000-app catalog, import and long messages. Layout regressions also check all five settings pages at 360 and 480 logical pixels wide. Screenshots use fictional profiles and a supplied desktop diagram. These checks verify the rendered application, not a physical Explorer desktop.

A separate local upgrade fixture starts the published 0.2.2 executable, then opens 0.2.3 with the same isolated profile. It verifies replacement of the running worker, the installed executable hash, retention of settings and usage history, an untouched user file, and a byte-identical backup of the previous executable.

Release packaging excludes installed profiles, usage databases and undo backups. Build-machine source paths are remapped in the release executable.

## Limits

Windows Server CI and Wine do not establish interactive acceptance on every Windows 10/11 desktop. Physical multi-monitor/DPI changes, alternative shells, third-party desktop organizers, ARM64 and future taskbar formats have not been established for this release. The 100 geometry combinations verify placement calculations, not universal hardware compatibility. Version **0.2.3 remains a preview**.
