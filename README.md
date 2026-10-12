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

1. Download and extract **DesktopPilot-0.1.0-windows-x64.zip** from [Releases](https://github.com/TechWisard/desktop-pilot/releases/latest).
2. Run `desktop-pilot.exe`. Choose **English** or **Русский**, review the shortcut area and startup preferences, then select **Start using Desktop Pilot**.
3. Use your computer normally. Suggestions need three distinct days of use. To add an app immediately, select **Always on desktop** in the Apps tab and choose **Apply now**.

Installation is per user in `%LOCALAPPDATA%\DesktopPilot`; no administrator access is required. Settings are available from the tray icon and the Start menu. This preview executable is unsigned; download only from this repository's Releases and verify the supplied SHA-256 checksum.

## A quieter desktop

| Behavior | Default after setup |
| --- | --- |
| Quick-access area | 4 shortcuts in a free area of the primary display |
| Review interval | Every 7 days, after an idle period and outside fullscreen activity |
| Ordinary replacements | At most 1 per review |
| Evidence required | At least 3 distinct active days |
| Replacement threshold | 1.5× the existing score **and** at least 3 points more |
| Recency | Scores halve every 14 days; usage totals retained for 90 days |
| Taskbar shortcuts | Already pinned apps excluded unless explicitly requested |
| Manual choices | Always / Automatic / Never; moved quick shortcuts stay pinned |

The first fill and explicit rule changes may affect more than one shortcut. Deleting a managed shortcut prevents it from coming back automatically. Files you edit are released from management. Each change has a local journal and an **Undo** action; rollback refuses to overwrite changed files.

## You decide what deserves a shortcut

Usage is more than a launch count. Repeated active days, foreground sessions, recency, explicit quick-shortcut launches, taskbar pins and manual choices contribute to the decision. If an app is normally opened through a file association, a context menu or a hotkey, choose **Never add**. Desktop Pilot cannot reliably infer that preference from process activity alone.

The catalog scans desktop shortcut folders up to three levels deep. Add other `.lnk` or `.url` files through Settings; it does not add every installed program. Ambiguous executable profiles are kept separate. Steam game directories and Windows app identities are used where they can be matched uniquely.

## Local by design

No account, analytics, cloud synchronization or network service. The tracker stores local app paths, daily usage totals, preferences and undo records. It does not read keystrokes, window titles, document content or browser history. **Do not upload your local database or undo folder.** [Privacy details](docs/privacy.md)

## Compatibility and verification

Version **0.1.0 is a preview**, intended for Windows 10/11 x64 with the standard Explorer desktop. Live desktop integration has been checked on Windows 11. Automated geometry tests cover 100 combinations of resolution, scaling and desktop origin, including empty, occupied and insufficient-space cases. Those tests do not replace physical multi-monitor testing.

Explorer auto-arrange must be off for positioning. A display-layout change pauses placement until you accept the new geometry. Unrecognized taskbar data stops changes rather than guessing; the pin filter can be disabled explicitly. Alternative shells and virtual-desktop managers are not validated. See [verification scope](docs/verification.md).

## Build

Install Rust **1.93.1** with the MSVC toolchain and Visual Studio C++ Build Tools, then run:

```powershell
pwsh -File scripts/build-release.ps1
```

The script runs formatting, unit tests and Clippy, then builds a release with local source paths remapped. The executable is in `target/release/desktop-pilot.exe`. The app is written in **Rust**, using egui/eframe, SQLite and native Windows Shell APIs.

For an isolated native integration check on an interactive Windows desktop:

```powershell
target/release/desktop-pilot.exe --self-test --home "$env:TEMP\test-desktop-pilot-unique" --out "$env:TEMP\desktop-pilot-test.json"
```

Use a new `test-*` directory each time. This test launches only its own inert fixture, never your apps. [Architecture, operation and removal](docs/guide.md)

## Feedback

[Report a bug](https://github.com/TechWisard/desktop-pilot/issues) with the Windows version, display scaling and reproduction steps. Remove personal paths and application history from any report or screenshot.
