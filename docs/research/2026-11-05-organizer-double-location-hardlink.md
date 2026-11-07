# One original at two recorded locations — 2026-11-05

## Source-confirmed boundary

The draft organizer's committed Undo preflight treats an original as already manually restored when its original desktop path matches the journaled filesystem ID. It skips checking whether the categorized path **also** still names the same file. The per-item Undo has the same shortcut. During interrupted recovery, Accept current locations likewise prioritizes the desktop path and marks the transaction Accepted.

Windows NTFS permits two hard links to represent one file in separate directories. Thus a shortcut hard-linked back to its desktop name without renaming out of the category folder satisfies the old "manually restored" ID check. Undo could mark its journal Undone even though the categorized entry still existed; Accept could clear a recovery barrier and update the catalog while the same original was left in both places. The risk is a misleading history and unexpected duplicate shortcuts, not overwrite of either object.

This is a source-grounded possibility and a reproducible synthetic filesystem scenario. No actual affected Desktop Pilot user or incidence rate has been established.

## Contracts and earlier evidence

- Microsoft, [Hard Links and Junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions): distinct NTFS paths can reference one underlying file; editing through either path affects both.
- Microsoft, [CreateHardLinkW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createhardlinkw): hard links are supported on NTFS for files on the same volume.
- Microsoft, [BY_HANDLE_FILE_INFORMATION](https://learn.microsoft.com/en-us/windows/desktop/api/fileapi/ns-fileapi-by_handle_file_information): the volume serial and file index jointly distinguish files; nNumberOfLinks describes multiple names.
- Earlier project studies in [2026-10-26-adaptive-shortcuts.md](2026-10-26-adaptive-shortcuts.md) describe why stable user-controlled placement matters. Existing [manual Undo](2026-11-05-manual-undo-restore.md) and [interruption recovery](2026-11-05-organizer-accept-unverified-original.md) regressions establish expectations for safe reconciliation without overwrites. No general user-research search was repeated.

## Change

The real filesystem-ID check now refuses an "already restored" original when its organized name **also** matches that original ID. The check runs before a committed Undo starts moving other objects and again in the per-file recovery path. Interrupted Accept rejects the same ambiguous state within its existing SQLite transaction; the journal, catalog and recovery barrier remain intact. The message asks the user to resolve the duplicate link and retry. Neither hard link is deleted, no new setting or telemetry is added, and manually renamed/moved originals continue to be accepted.

## Windows regressions on production paths

1. Create two real .url shortcuts and record file IDs. Organize them using the actual Plan, same-volume file renames, journal and SQLite catalog.
2. Create an NTFS hard link for one organized original at its old desktop name. Assert Undo refuses the request **before** moving the second original or changing the catalog/journal; no pending recovery barrier is spuriously installed.
3. Delete only the test-created extra hard link and retry Undo; both originals restore with their initial file IDs and no organized duplicates.
4. Simulate an actual partially applied journal, then hard-link its first relocated original back to the desktop. Accept current locations must preserve the pending journal and unchanged catalog.
5. Remove only the test link, retry Accept and verify correct, identity-checked paths and cleared pending state.

These are correctness assertions on synthetic NTFS files invoking production logic, not measured human productivity or an interactive Windows 11 Explorer session.

## Release boundary

Run unchanged strict Windows Server 2022/2025 CI: formatting, locked Rust tests, Clippy -D warnings, locked build, native checks and release guards. Keep the organizer draft; physical Windows 11/OneDrive/redirected desktop, 100–200% DPI, cloud sync, and genuine crash-resume scenarios remain release gates. No separate public CI stand or product release is needed for this draft change.
