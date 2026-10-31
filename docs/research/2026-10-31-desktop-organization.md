# Explicit, reversible desktop organization

## Status

Work in progress on `feature/desktop-organization-20261031`, based on `87ac2c95c5c0e9fa7773c5c53a7668e35e464baf` (0.2.76). The filesystem engine is initially compiled by the existing test target. The user interface, first-run entry, worker quiescence and final integration are not yet exposed. This document does not claim a successful test run or a new release.

## User problem and intended contract

A full desktop must not force a new user to manually clear an area before receiving any benefit. Offer a read-only plan, ordinary category folders and explicit Apply. Default to shortcuts. Moving independent documents/media requires a separate opt-in and preview; unknown dependencies are not guessed. Existing directories/projects, public desktop objects, managed quick copies, Always/Never rules, hidden/system/reparse/cloud-placeholder files remain untouched.

Research already accumulated in `2026-10-26-adaptive-shortcuts.md` supports predictability and user control, not automatic rearrangement of every object. The CHI study *Predictability and Accuracy in Adaptive User Interfaces* reports satisfaction effects for its experimental interfaces; it does not establish human time savings for this implementation:
https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/

Windows shortcut metadata has separate targets, arguments, working directories and icons. Read that metadata without resolving targets, opening applications or probing remote destinations. Reject relative/uncertain targets, preserve distinct launch profiles, and re-read launch identity after a relocation:
https://learn.microsoft.com/en-us/windows/win32/shell/links

## Filesystem and recovery design

- Plan against the current personal desktop, actual Explorer geometry and saved rules. Do not recurse into existing user folders. Scope is a local desktop; redirected network roots are refused explicitly.
- Classify recognized shortcuts into Games, Programs and Sites. Optional ordinary-file categories are conservative. Spreadsheets, scripts, executables, associated file sets and known shortcut dependencies are not moved.
- Do not create singleton folders. Count new folders themselves in the preview's icon reduction. Reuse only folders whose persisted filesystem identity matches; a foreign folder with the same category name is never merged.
- Apply validates the entire plan before mutation. Use the application's changes lock, a per-desktop lock and the background singleton. The GUI must stop the existing worker before calling Apply and resume only a previously enabled worker.
- A durable SQLite intent record precedes the first directory creation/rename. Temporary catalog-write triggers block stale scanner/import/rule commits while the filesystem transaction is incomplete, including after a crash. Dropping the triggers, updating only catalog source paths and completing the journal is one SQLite transaction.
- Move by the open source handle to the verified destination-directory handle. ReplaceIfExists is false. There is no copy/delete fallback, cross-volume move or recursive deletion. Original bytes, alternate data streams and file identities are retained by a same-volume rename.
- Undo uses identity, not just matching bytes. Preserve edits to the same file; refuse a replacement. A newly occupied original name is never overwritten. Conflicts remain recoverable; explicit acceptance keeps current files and retains the historical journal.
- Delete only positively identified, empty category directories created by the operation. A crash between mkdir and recording directory identity leaves that unverified folder alone.
- Restore positions only for moved originals/new category folders, not unrelated icons the user may have moved later. Occupied locations or changed monitor topology produce a warning rather than overlap or forced repositioning.

Primary filesystem contracts:
https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info
https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle

## Reproducible fixtures

`src/organize/tests.rs` uses the real SQLite Store, Windows `.lnk` parser, `.url` files and Windows handle-based renames in isolated temporary directories. It covers read-only preview, apply/undo, failed writes, interrupted renames/reopen, collisions, replaced files/folders, same-file edits, profile separation, cancellation, manual exclusions, managed copies, existing placement journals, dependency protection and folder-aware layout projections. No real desktop is mutated and no test shortcut target is launched.

## Acceptance still required

1. Run the unchanged Windows checks, resolve compiler/format/lint/runtime failures and record the actual results.
2. Complete the readable RU/EN preview/selection/recovery interface and first-run flow; test at 100% and 200% scale.
3. Exercise worker shutdown/resume and concurrent settings against the real public entry points. Do not silently enable observation or Windows startup.
4. Verify actual quick-placement/Undo integration after originals move and return; preserve policies/history.
5. Only publish a version after all gates pass and the tag, source commit and distribution files agree.

External document dependencies, cloud synchronization effects, physical multi-monitor Windows 11 behavior and human productivity remain outside what isolated filesystem fixtures prove. No claim of absolute safety against disk failure or of measured human time savings is made.
