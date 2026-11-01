# Prevent accidental script replacement by JSON report export — 2026-11-02

## Why this matters

Desktop Pilot's direct commands can export their outcome with `--out PATH`. A user may choose a filename already used by a local backup, startup script or registry setup. An ordinary report is JSON text, not a runnable program, and publishing it over a script would destroy the script's content. Reports should preserve user files and make an invalid destination actionable **before** placement or Undo.

Continue the previous [predictable user-control research](2026-10-26-adaptive-shortcuts.md) and [interruption/rollback validation](2026-11-01-pending-manual-policy-recovery.md). Findlater and McGrenere's [CHI 2004 study](https://doi.org/10.1145/985692.985704) motivates predictable control but is **not** a measurement of Desktop Pilot report errors. Microsoft's [Windows naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) document special device names and Windows filename semantics. Extension-based executable/script formats are an additional *application safety policy*, not a universal Windows filesystem restriction. No field incident frequency or user-time savings is known.

## Confirmed source behavior and correction

At main `8fbe2a61f17749ea01abfba59138361f357d4784`, `report::validate_name` rejected databases, `.exe`, `.dll`, `.lnk`, and `.url` but **accepted** existing `.cmd`, `.bat`, `.ps1`, `.vbs`, `.hta`, `.msi` and `.reg` paths. `report::write` then used a durable temporary file followed by `MOVEFILE_REPLACE_EXISTING`, so a user-provided report path could atomically replace the script. This is a confirmed source-path hazard, not an observed incident.

Reject common Windows executable, script, installer, and registry file extensions (case-insensitive) during both `preflight` and `write`. Keep ordinary JSON destination names, existing safe report-overwrite semantics, atomic publication, error handling and original file/SQLite protections unchanged. No new settings, permissions, dependencies or telemetry.

## Production-path regression and related scenarios

`report::tests::existing_script_files_survive_report_preflight_and_write` writes eight synthetic, pre-existing files in an isolated temporary directory, then calls the same `preflight` and `write` functions used by direct CLI reporting. Each rejected path must retain its original bytes, while `audit.json` must successfully publish a JSON report. The directory must contain no temporary report residue. The existing `report_cannot_replace_profile_files_or_shortcuts` test covers the additional extensions, including uppercase names, and ordinary database, shortcut and DOS-device protections.

This change does not try to identify all user-sensitive documents or infer file contents from names. A deliberate existing `.json` report remains replaceable by design. It also does not change the layout, recommendation ranking or file modification stages of Apply and Undo.

## Verification and continuation

Run unchanged Windows Server 2022/2025 formatting, locked Rust tests, strict Clippy, locked build, native fixture suite and publication guards. The tests validate file safety on isolated fixtures, not UI at 200% scaling or physical Windows 11. Do not release until required CI and source/release consistency are verified.
