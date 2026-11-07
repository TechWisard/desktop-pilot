# Preserve literal-percent custom shortcut icon files — 2026-11-07

## Observation and scope

Microsoft documents a separate stored icon path and index in Windows Shell links via [IShellLinkW::SetIconLocation](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-seticonlocation) and [GetIconLocation](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-geticonlocation). A user can keep a custom shortcut icon as an ordinary file on their Desktop. The optional organizer can move image files after explicit consent.

The source previously indexed only the environment-expanded form of saved target, working-directory and icon paths. When the saved icon filename literally contains an existing environment-variable pattern, for example `Cover%SystemRoot%.png`, expansion silently changes the path in the dependency index. The actual Desktop filename is then not protected. A moved icon file can leave the existing shortcut with a stale custom-icon path.

This is a source-confirmed boundary defect, not a measured user incident. [Reports of disappearing/custom shortcut icons in desktop organizers](https://forums.stardock.com/509371/some-applications-shortcut-icons-in-fences-suddenly-become-blank-icons) establish the general user-visible cost, **not** that this specific Desktop Pilot scenario occurred there. Existing [research on predictability and user control](2026-10-26-adaptive-shortcuts.md) remains applicable.

## Narrow correction

Index both literal and environment-expanded spellings for local launch targets, working directories and icon paths. A false-positive dependency can keep one file on Desktop, which is preferable to silently breaking an existing shortcut. Do not launch the target, resolve arbitrary remote paths, rewrite user shortcuts, change consent, add telemetry or move the file.

## Reproduction on production code

`literal_percent_custom_shortcut_icon_stays_with_its_link_after_organization` creates a real `.lnk` via Windows Shell-link COM, an independent executable fixture and three ordinary Desktop PNGs. It uses the real Settings-backed SQLite catalog and production `analyze`, `apply`, `undo`.

Acceptance: the previously issued stale preview is rejected; the literal-percent icon and link bytes remain; the analyzer shows a dependency reason; **both** independent PNGs move after explicit consent and return with Undo; there is no pending journal and the catalog is unchanged. No external files, launch target or user Desktop are touched.

**Synthetic before/after:** under the old path index the actual literal-percent icon path is absent, while under the corrected index it is protected. Intended change: 1 fewer incorrect proposed move without losing 2 useful independent moves; user productivity has not been measured.

## Verification and remaining work

Run unchanged format, all locked Rust tests, strict Clippy, build, native integration and Windows Explorer acceptance on the proposed commit. The icon-specific test verifies metadata and preservation, **not** real Explorer icon rendering on a user's customized desktop. OneDrive, interactive scaling at 100%/200%, long-running human usage and feature-level release acceptance remain independently gated.
