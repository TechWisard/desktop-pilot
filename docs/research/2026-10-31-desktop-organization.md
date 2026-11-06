# Explicit, reversible desktop organization

## Status

Work in progress on `feature/desktop-organization-ui-20261031`, based on the earlier engine branch `feature/desktop-organization-20261031`, branched from 0.2.76. The feature is now integrated into the main executable and Settings: a read-only plan, explicit preview/selection, manual apply, Undo, recovery, worker quiescence and first-run cleanup entry. A strict compilation, native runtime and UI acceptance gate is required before merging; this document does not claim a release.

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

## Windows-specific regressions (October 31)

The first native CI build reproduced `ERROR_INVALID_PARAMETER (0x80070057)` when
`SetFileInformationByHandle(FileRenameInfo)` was given a verified destination
directory handle and a relative leaf name. The Windows-compatible fix uses
`NtSetInformationFile` with the same existing open handles, no replacement,
and an explicit NTSTATUS-to-Win32 error translation. The real Windows Server
2022 test rerun at
https://github.com/TechWisard/desktop-pilot/actions/runs/41202182349
completed 366 of 367 Rust tests; 13 earlier failing move/undo/recovery tests
passed after this change. The remaining failure concerned layout projection,
not the rename primitive. The normal CI gate remains enabled.

The layout failure exposed a real path-identity distinction: Explorer supplied
the user's desktop under the shortened `RUNNER~1` path while the working folder
was canonicalized to the long `Runneradmin` spelling. Simple lowercase
normalization cannot prove that those names refer to the same file. The
analyzer now indexes positions once and uses a verified file ID only when
the fast path comparison misses. The projected-space function likewise
removes only verified moved objects, not unrelated icons sharing a basename.
The regression includes a separate same-name foreign item that must remain
occupied. This fix requires a successful Windows CI rerun before acceptance.

Supporting primary Windows API documentation:
https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle

## Integration progress and verified boundaries (October 31)

- Existing isolated Windows test fixtures are preserved. A public Windows CI run on the integrated branch passed **390 Rust tests on each Windows Server 2022 and 2025 runner** before failing on strict lint, primarily because the original filesystem test shim was mistakenly imported a second time. The duplicate module was removed; see [the recorded failing run](https://github.com/TechWisard/desktop-pilot/actions/runs/41204892746). This is a synthetic regression count, not a usability measurement.
- First-run Settings now offers desktop organization before placement. Ordinary objects are moved only after explicit Apply; documents/media are opt-in. Preview offers per-item exclusions, a filtered list, a projected freed-space count and an interrupt-recovery path.
- The organizer now runs with the startup mutex held while the background worker is stopped. A formerly active worker restarts after the journal is resolved; stopped profiles remain stopped. Startup refuses a pending recovery, including after a process crash.
- UI state and functional journal are kept distinct; no automatic recurring cleanup or hidden telemetrics were introduced. Remaining acceptance: final green CI after integration, read-through of static UI at 200% and an actual Windows 11 desktop operation with rollback.

## Reproducible fixtures

`src/organize/tests.rs` uses the real SQLite Store, Windows `.lnk` parser, `.url` files and Windows handle-based renames in isolated temporary directories. It covers read-only preview, apply/undo, failed writes, interrupted renames/reopen, collisions, replaced files/folders, same-file edits, profile separation, cancellation, manual exclusions, managed copies, existing placement journals, dependency protection and folder-aware layout projections. No real desktop is mutated and no test shortcut target is launched.

## Acceptance still required

1. Verify the integrated Windows checks on both CI environments without loosening test, lint or build gates.
2. Test the full RU/EN preview, scrolling, action confirmation and recovery at 100% and 200% scale in the running GUI.
3. Exercise worker shutdown/resume and concurrent settings on the actual Windows desktop and confirm existing observation/startup preferences are preserved.
4. Verify actual quick-placement/Undo integration after originals move and return; preserve policies/history.
5. Only publish a version after all gates pass and the tag, source commit and distribution files agree.

External document dependencies, cloud synchronization effects, physical multi-monitor Windows 11 behavior and human productivity remain outside what isolated filesystem fixtures prove. No claim of absolute safety against disk failure or of measured human time savings is made.
