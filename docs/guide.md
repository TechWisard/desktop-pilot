# User guide

[Русский](guide.ru.md) · [Home](../README.md)

## First launch

The welcome page lets you choose the interface language, automatic changes, Windows startup, taskbar filtering and a free area for 1–8 shortcuts. Opening the welcome page alone does not start tracking or configure startup. **Start using Desktop Pilot** installs a per-user executable, scans shortcuts and starts the tray process. You can use tracking without automatic changes.

The app uses Windows Known Folders rather than hardcoded account paths, including redirected desktops. It scans the current user's desktop, including shortcut folders up to three levels deep. Reparse points, common source/dependency folders and shortcut archives are skipped. Public Desktop-only shortcuts and Start menu shortcuts can be added explicitly from the Apps tab. It does not automatically inventory or publish installed software.

## Everyday use

- **Overview:** see managed shortcuts, apply the current selection, or undo the previous change.
- **Apps:** inspect ranking reasons, refresh the catalog, add an existing shortcut, and set Always / Automatic / Never.
- **Settings:** switch English/Russian, pause collection or changes, adjust review timing and the shortcut area, accept a changed display geometry, or clear usage history.
- **History:** review changes and errors. Records stay local.

Save settings after changing them. Manual rules are submitted immediately to the background process; use Apply now to change the shortcut selection. A rule change or slot reduction may remove more than the ordinary replacement limit. More manual pins than available slots blocks changes until you resolve the conflict.

Moving or renaming a managed quick shortcut pins it at your chosen position. Deleting or moving it out of the desktop excludes it from automatic return. Editing its file content releases ownership. The tracker checks these changes approximately once per minute. Existing category folders are not reorganized.

## How ranking works

For each local calendar day:

```text
daily score = 5 × min(quick launches, 20)
            + 2 × min(foreground sessions, 12)
            + 0.4 × ln(1 + min(active seconds, 3600) / 60)
weight      = 2 ^ (-age in days / half-life in days)
total       = sum(daily score × weight)
```

Foreground observations are sampled every two seconds. A qualifying session needs approximately ten seconds of activity; sessions within twenty minutes are deduplicated. Idle periods and long gaps between observations are not counted as active work. Default idle cutoff: 120 seconds.

Foreground use does **not** prove a shortcut click. Explicit launches through managed quick shortcuts are counted separately. Two shortcuts using the same executable but different arguments or working directories are distinct identities; ambiguous foreground matches are skipped. Steam games can match a unique installation directory; Windows app shortcuts can match a unique application ID.

Fresh profiles wait for three distinct active days. The first eligible fill can use all available slots. Subsequent automatic reviews run every seven days, after at least fifteen seconds of user inactivity, outside fullscreen activity and mouse dragging. Default ordinary replacement limit: one; a challenger must exceed the current score by both 1.5× and 3 points. Existing valid shortcuts do not disappear just because history was cleared or aged out.

Manual choices override ranking. Taskbar pins are excluded by default. Context-menu, file-association and hotkey preferences must be expressed with **Never add**. The app cannot determine those preferences reliably on its own.

## Placement and recovery

The shortcut area uses Explorer's grid on the primary monitor. Initial setup finds an unoccupied area; existing items are included in the positioning batch so that Explorer does not shift them. Full grid cells are reserved, including space for captions. Long quick-shortcut names are shortened; the original name and shortcut are retained in the catalog.

Display geometry changes, unexpected taskbar formats, an occupied area or an interrupted transaction pause placement. Fix the reported condition, or explicitly disable the taskbar filter when appropriate. **Accept current display geometry** acknowledges the current display layout; it does not move existing icons.

Each transaction records before/after state and backup bytes before changing files. Undo checks hashes before removing or restoring a file. If a user edited a file, it stops instead of overwriting the edit. Undo pauses automatic changes. Original applications are launched through their original Shell shortcut, preserving its arguments, working directory and elevation behavior.

## Disable or remove

Extract the release archive and run:

```powershell
powershell -NoProfile -File .\disable.ps1
```

This stops the background process, disables tracking and startup, and undoes managed changes. `-KeepShortcuts` disables the tracker while retaining quick shortcuts; those shortcuts still require the installed executable and database to launch.

Local data is retained for recovery. After undo completes successfully and no managed shortcuts remain, you can remove `%LOCALAPPDATA%\DesktopPilot` and the `Desktop Pilot` Start menu shortcut. Never delete that directory while keeping managed quick shortcuts. If undo reports a conflict, resolve it before removing the application.

## Update

Close Settings, use **Exit** in the tray menu, and back up the local data directory after the process stops. Replace only `desktop-pilot.exe` in `%LOCALAPPDATA%\DesktopPilot` with the new release executable, then run it. Configuration and history live separately from the executable. Updates are manual; there is no background downloader.

## Source layout

| Module | Responsibility |
| --- | --- |
| `model.rs` | Identity, configuration, ranking and stable selection |
| `native.rs` | Windows Shell, desktop geometry, activity and taskbar integration |
| `engine.rs` | Catalog, ownership, transactions, rollback and observation loop |
| `store.rs` | Local SQLite persistence |
| `setup.rs` | Per-user first-run installation |
| `ui.rs`, `i18n.rs`, `tray.rs` | Bilingual settings and tray controls |
