# Desktop Pilot

**Adaptive desktop shortcuts for Windows. Keep useful apps close and your existing layout intact.**

[Download for Windows](https://github.com/TechWisard/desktop-pilot/releases/latest) · [Русский](README.ru.md) · [How it works](docs/guide.md) · [Privacy](docs/privacy.md)

![Windows checks](https://github.com/TechWisard/desktop-pilot/actions/workflows/checks.yml/badge.svg)
![Platform: Windows x64](https://img.shields.io/badge/platform-Windows_x64-0078D4)
![Interface: English + Русский](https://img.shields.io/badge/interface-English_%2B_Русский-46BBA0)

Desktop Pilot is a local Windows desktop organizer and shortcut manager. It learns which apps you use, then maintains a small, stable group of quick-access shortcuts. Your original shortcuts, category folders and manually chosen positions stay under your control.

![Desktop Pilot overview, using fictional demonstration data](docs/images/overview-en.png)

*The screenshot uses fictional demonstration data.*

## Get started

1. Download and extract **DesktopPilot-0.2.1-windows-x64.zip** from [Releases](https://github.com/TechWisard/desktop-pilot/releases/latest).
2. Run `desktop-pilot.exe`. The setup walks through the operating mode, number and position of shortcuts, and observation preferences. Choose **English** or **Русский** at any time.
3. Use your computer normally. The default is **passive observation for 7 computer sessions**. No quick shortcuts are created during learning. After the threshold, the most-used eligible apps appear automatically during an idle period. Choose sessions, active days, either condition or both in Settings.

Installation is per user in `%LOCALAPPDATA%\DesktopPilot`; no administrator access is required. Settings are available from the tray icon and the Start menu. This preview executable is unsigned; download only from this repository's Releases and verify the supplied SHA-256 checksum.

## Easier to use in 0.2.1

- Larger text, generous buttons, clear navigation and a saved 100% / 125% / 150% interface scale.
- Three understandable modes: automatic adaptation, observation with manual changes, or full manual control.
- Pick a shortcut area on a desktop map, or let the app find free space. Advanced tuning is grouped separately.
- Preview which apps will be added or removed before applying. Unsaved settings are clearly distinguished from the running configuration.
- Search and filter the app list, inspect each selection reason, and set a rule without navigating a dense table.
- Open the downloaded executable to update an existing installation. The background process is replaced safely and your profile is retained.

## A quieter desktop

| Behavior | Default after setup |
| --- | --- |
| Quick-access area | 4 shortcuts in a free area of the primary display |
| Review interval | Every 7 days, after an idle period and outside fullscreen activity |
| Ordinary replacements | At most 1 per review |
| Initial observation | 7 qualified computer sessions; configurable sessions / active days / either / both |
| Replacement threshold | 1.5× the existing score **and** at least 3 points more |
| Recency | Scores halve every 14 days; usage totals retained for 90 days |
| Taskbar shortcuts | Already pinned apps excluded unless explicitly requested |
| Manual choices | Always / Automatic / Never; moved quick shortcuts stay pinned |

A computer session needs at least 5 minutes of tracked activity; a new session starts after 30 minutes without it. An active day also needs 5 minutes. Restarting the app preserves progress. Thresholds apply to the initial learning phase; after completion, ordinary weekly reviews take over.

The first fill and explicit rule changes may affect more than one shortcut. Deleting a managed shortcut prevents it from coming back automatically. Files you edit are released from management. Each change has a local journal and an **Undo** action; rollback refuses to overwrite changed files.

## You decide what deserves a shortcut

Usage is more than a launch count. Repeated active days, confirmed foreground sessions, recency, taskbar pins and manual choices contribute to the decision. Opening a process or calling a shortcut adds no ranking points by itself. A foreground window must receive fresh keyboard, button or wheel interaction before its use counts; Windows-marked injected input is excluded. Startup apps can qualify only when you actually interact with them. If an app is normally opened through a file association, a context menu or a hotkey, choose **Never add**. Desktop Pilot cannot reliably infer that preference from process activity alone.

The catalog scans desktop and Start menu shortcuts up to five folder levels deep. Add other `.lnk` or `.url` files from Apps. Catalog discovery does not put every application on the desktop. Ambiguous executable profiles are kept separate. Steam game directories and Windows app identities are used where they can be matched uniquely.

Quick shortcuts created with 0.1.1 or later are exact copies of your originals. They launch directly and keep working when Desktop Pilot is stopped or its database is unavailable.

## Local by design

No account, analytics, cloud synchronization or network service. The tracker stores local app paths, daily usage totals, preferences and undo records. It never records key codes, typed text, window titles, document content or browser history. Input provenance flags and a recent interaction timestamp are used to distinguish activity from unattended launches. **Do not upload your local database or undo folder.** [Privacy details](docs/privacy.md)

## Compatibility and verification

Version **0.2.1 is a preview**, intended for Windows 10/11 x64 with the standard Explorer desktop. The release is checked on Windows Server 2022 and 2025 runners, with separate application screenshots and isolated filesystem fixtures. The current verification scope is recorded in the linked report. Automated geometry tests cover 100 combinations of resolution, scaling and desktop origin, including empty, occupied and insufficient-space cases. Those tests do not replace physical multi-monitor testing.

Explorer auto-arrange must be off for positioning. A display-layout change pauses placement until you accept the new geometry. Unrecognized taskbar data stops changes rather than guessing; the pin filter can be disabled explicitly. Alternative shells and virtual-desktop managers are not validated. See [verification scope](docs/verification.md).

## Build

Install Rust **1.93.1** with the MSVC toolchain and Visual Studio C++ Build Tools, then run:

```powershell
pwsh -File scripts/build-release.ps1
```

The script runs formatting, unit tests and Clippy, then builds a release with local source paths remapped. The executable is in `target/release/desktop-pilot.exe`. The app is written in **Rust**, using egui/eframe, SQLite and native Windows Shell APIs.

For isolated native integration checks:

```powershell
target/release/desktop-pilot.exe --self-test --home "$env:TEMP\test-desktop-pilot-unique" --out "$env:TEMP\desktop-pilot-test.json"
```

Use a new, empty `test-*` directory each time. The fixture uses its own shortcut directory and launches only its own inert executable. It does not change the real desktop or startup entries. [Architecture, operation and removal](docs/guide.md)

## Feedback

[Report a bug](https://github.com/TechWisard/desktop-pilot/issues) with the Windows version, display scaling and reproduction steps. Remove personal paths and application history from any report or screenshot.
