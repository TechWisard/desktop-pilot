# Desktop Pilot

**Useful shortcuts nearby. Desktop organization that stays under your control.**

[Download for Windows](https://github.com/TechWisard/desktop-pilot/releases/latest) · [Русский](README.ru.md) · [User guide](docs/guide.md) · [Privacy](docs/privacy.md)

![Windows checks](https://github.com/TechWisard/desktop-pilot/actions/workflows/checks.yml/badge.svg)

Desktop Pilot is a local Windows desktop organizer and shortcut manager. It has two separate jobs: maintain a small, stable set of quick-access shortcut **copies**, and **organize existing desktop items only when you explicitly ask**. Neither action replaces your manual choices with an unexplained automatic cleanup.

## Get started

1. Download `DesktopPilot-*-windows-x64.zip` from the [latest release](https://github.com/TechWisard/desktop-pilot/releases/latest), compare its SHA-256 with `SHA256SUMS.txt`, and extract it.
2. Run `desktop-pilot.exe`. Choose English or Russian and follow setup. Automatic adaptation, observation with manual changes, and fully manual control are separate choices.
3. To organize existing desktop shortcuts, open **Usage → Organize desktop**, review the proposed groups, leave anything you need in place, then apply the plan.

To update an existing installation, open the executable from the new archive. Your local profile, rules and history are retained. The executable is unsigned; verify its source and checksum before opening it.

**Development branch:** The desktop organization feature described below is not included in the currently published 0.2.127 download. It is undergoing final acceptance here.

## Stable shortcut search fixes retained in this branch

## Improvements in 0.2.127

**Copied web root addresses now find their saved shortcut even with an optional `/` before a query or fragment.** The lookup preserves distinct HTTPS/HTTP destinations, query spelling, source files and **Always** / **Never** rules; unrelated notes are never accepted as an exact address hit. This correction is inherited from the [published stable release](https://github.com/TechWisard/desktop-pilot/releases/tag/v0.2.127), with [real shortcut and SQLite regression](docs/research/2026-11-07-http-root-lookup.md). Desktop organization remains unreleased in this draft.

## Improvements in 0.2.126

**Full Windows launch paths now preserve the right shortcut, not another project's lookalike.** If a folder is literally named `%USERNAME%`, it is distinct from the account-name folder. Previously these different application targets and their working directories could be assigned the same identity; native availability checks could also inspect the wrong file. The fix preserves separate **Always** / **Never** choices and correctly recognizes offline and restored targets. Relative `%SystemRoot%` launch templates continue to work. Existing shortcuts, layout, profiles and usage history are not rewritten. [Filesystem, SQLite and selection regression](docs/research/2026-11-07-absolute-target-identity.md).

The desktop organization feature remains under acceptance in [PR #211](https://github.com/TechWisard/desktop-pilot/pull/211); this release does **not** include it.

## Improvements in 0.2.125

**Pasting a complete path to an uncommon Windows command target now selects the correct saved launch profile.** Full paths to .com, .vbs, .vbe, .js, .jse, .wsf, .wsh and .msc now use the same exact target matching as .exe, .cmd and .bat. This prevents misleading matches from unrelated notes and backup filenames, including under **Always** / **Never** filters. Ordinary word searches remain broad. No application is executed during search, and existing files, policies, history and desktop positions are unchanged. [Production catalog and SQLite regression](docs/research/2026-11-07-windows-command-target-lookup.md).

The separate **Organize desktop** feature is still under acceptance in [PR #211](https://github.com/TechWisard/desktop-pilot/pull/211) and is **not** included in this release.

## Improvements in 0.2.124

**Pasting a full `.cmd` or `.bat` launch path now finds the correct saved shortcut and policy.** Previously a complete path was treated as general text and could also match another shortcut mentioning that path in its note or having a longer filename. Exact Windows batch-launcher lookup now respects the same rules as `.exe` paths, including **Always** and **Never** filters, while ordinary name/folder search stays broad. Saved shortcuts, launch settings, desktop positions and usage history remain unchanged. [Reproducible Windows/SQLite scenario](docs/research/2026-11-07-copied-batch-targets.md).


## Improvements in 0.2.123

**A copied full shortcut path now selects the correct app and its saved rule.** Different folders can contain same-named shortcuts, including a folder literally called `%USERNAME%` and one named after the Windows account. In Apps search, actual absolute filenames are no longer expanded a second time, which could display both saved entries instead of the one selected in Explorer. Existing **Always** and **Never** rules stay distinct; abbreviated `%SystemRoot%` queries still work. Shortcuts, history, layout and preferences are not modified. [Real shortcut and SQLite regression](docs/research/2026-11-06-literal-copied-path-search.md).

## Organize the desktop deliberately

The organizer first produces a read-only preview. By default it proposes groups for ordinary `.lnk` and `.url` shortcuts, not documents, projects or folders. Ordinary file categories require a separate opt-in. Groups with only one item do not create unnecessary folders.

Every proposed item has a checkbox. Search and pages help inspect large desktops; filtering does not secretly clear selections on other pages. **Remember exceptions** can save the decision to keep everything where it is, without moving a file or creating a fake Undo entry. Changed or replaced files invalidate the old plan before movement starts.

Original files are moved within the personal local desktop, not copied and deleted. Existing unrelated folders are never adopted, and conflicting names are never overwritten. Managed quick shortcuts, manual rules and known file dependencies remain protected. Cloud placeholders, reparse objects, executables, scripts and project folders are not silently reorganized.

Organization has its own **Undo** and persistent recovery journal. If the program is interrupted, reopen it and complete recovery. A manually returned original is recognized; its new icon position is not overwritten. A different file occupying the old name stops restoration rather than losing either file. See the [organization and recovery guide](docs/guide.md).

## Keep familiar shortcuts stable

Automatic adaptation uses qualified foreground activity, repeated use and recency, not raw process launches. Defaults are four quick shortcuts, a seven-day review interval and at most one ordinary replacement per review. Initial observation waits for seven qualified computer sessions; each requires five minutes of tracked activity. These settings are adjustable.

**Always / Automatic / Never** rules override ranking. Set the replacement limit to **0** to keep existing choices while allowing empty places to fill. Manually moved quick shortcuts remain anchored. Removing one prevents its automatic return; editing one releases it from management. Source outages do not immediately discard a working copy. Multiple launch profiles are kept distinct.

Automatic changes wait for actual inactivity, no held keys or mouse buttons, and a non-fullscreen foreground. The conditions are checked again before changes. The organizer itself is never scheduled automatically.

## Readable and local

English and Russian, saved text scale from 100% to 200%, keyboard zoom, large controls, searchable app details and separate advanced settings. A stopped profile is not silently restarted just because Settings is opened.

No account, telemetry, cloud synchronization or network service. Local records contain app paths, aggregate usage, preferences and Undo data, not typed text, key codes, window titles, document content or browsing history. **Do not upload your profile database or Undo folder.** [Privacy details](docs/privacy.md)

## Compatibility and verification

The application targets Windows 10/11 x64 with the standard Explorer desktop. This organizer's acceptance includes actual Windows 11 Explorer apply/Undo, Windows Server 2022/2025 checks, abrupt child-process termination followed by recovery, and native English/Russian interface captures at 100% and 200%, including scrolled content.

The hosted Windows 11 machine is ARM64 running the x64 executable under emulation; UI captures use an app-local software renderer. This is not a physical x64 GPU or multi-monitor certification. Layout, occupied-cell, negative-origin and topology-change regressions cover the production placement logic, but do not substitute for every hardware arrangement. Active OneDrive-account synchronization and alternative shells are not certified. Unsupported cloud/network objects are refused or skipped, not silently moved.

Explorer auto-arrange must be off for controlled positioning. Display-layout changes stop unsafe placement. [Release notes](docs/releases/0.3.0.md) · [Historical verification records](docs/verification.md)

## Build

Install Rust **1.93.1** with the MSVC toolchain and Visual Studio C++ Build Tools:

```powershell
pwsh -File scripts/build-release.ps1
```

The script checks formatting, tests and strict Clippy before building `target/release/desktop-pilot.exe`. The application uses Rust, egui/eframe, SQLite and native Windows Shell APIs. Third-party and build-dependency notices are retained in the repository and distribution.

For isolated native checks, use a new empty test profile:

```powershell
target/release/desktop-pilot.exe --self-test --home "$env:TEMP\test-desktop-pilot-unique" --out "$env:TEMP\desktop-pilot-test.json"
```

The fixture does not alter your real desktop or autostart. Live Explorer and Cloud Files acceptance are separately restricted to an explicitly disposable hosted test session.

[Report a problem](https://github.com/TechWisard/desktop-pilot/issues) with reproduction steps, Windows version and scaling. Remove personal paths and history from reports.
