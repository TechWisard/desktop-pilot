# External subtitle folders beside desktop videos — 2026-11-09

## Verified risk and basis

The optional ordinary-file organizer considers only immediate Desktop entries; its earlier same-directory sidecar guard cannot associate `Desktop/Episode.mp4` with `Desktop/Subtitles/Episode.eng.forced.smi`. The production analyzer and Apply use the same dependency snapshot, so both previously omitted this external name. This is a source-confirmed risk and a documented media naming convention, **not** a measured Desktop Pilot data-loss report.

Primary references: Plex's [local subtitle naming and subfolder guide](https://support.plex.tv/articles/200471133-adding-local-subtitles-to-your-media/) explicitly recognizes sibling `Subs` and `Subtitles` subdirectories; [Kodi external subtitles](https://kodi.wiki/view/Subtitles) independently documents file dependencies, with separate path rules. Microsoft documents [handle-bound directory enumeration](https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info). See also issue #262 and the already implemented sibling SAMI protection in #261.

## Proposed correction

For opt-in file organization only, inspect at most two recognized Desktop sibling subtitle folders using a verified **non-reparse directory handle** and bounded Win32 name enumeration (no media reads, recursive traversal, dynamic downloads, or symlink following). A subtitle matching a video stem, optionally with dotted language/presentation suffixes, keeps that video on Desktop. Ordinary files named `Subs`, unrelated subtitle names, and unrelated videos are left eligible. If folder enumeration cannot be trusted, keep all eligible videos, but continue allowing independent shortcuts and documents. Apply rebuilds the same snapshot before moving anything, rejecting stale plans.

## Production-path regression

`sibling_subtitle_directory_preserves_only_matching_videos_and_all_manual_choices`: real disposable Windows files, SQLite manual Always/usage history, production analyze/apply/undo; expected **before**: three videos offered and Episode may be moved alone; **after**: only two independent videos move into one Media folder and safely return on Undo. `late_subtitle_folder_or_subtitle_file_rejects_the_entire_old_plan` tests both arrival orders after Preview: no files or category folders may move. `unrelated_subtitle_directory_and_plain_file_named_subs_do_not_block_videos` guards overbroad exclusions; exact-name and dotted-suffix matching has a unit regression.

No user files are changed by Analyze, no new settings/permissions/telemetry introduced. Any measured desktop-slot difference is synthetic, not measured human time saved. Record actual locked Windows test, native and UI CI results on the exact proposed SHA before merging.

## Release boundary

This change remains on the unreleased organizer integration branch until the existing physical Windows 11 multi-monitor/DPI, live OneDrive account, keyboard/accessibility and power-interruption acceptance gates complete. Stable main, tags and published downloads remain unchanged.
