# Opt-in file organization: launch and icon paths with local aliases (2026-11-06)

## Why this matters

Desktop Pilot normally groups **recognized shortcuts only**. Moving ordinary images or documents requires a separate explicit choice. A saved application can open an image/document, or use a local icon file, through a Windows path spelling different from the desktop listing. If that file moves, the saved launch or icon reference can stop working. The user needs a dependable preview and a stale-preview refusal before any mutation.

**Source-confirmed gap, not a reported customer incident:** `Dependencies::record` previously added launch targets and icon files to a normalized-text set, and `Dependencies::check` compared the desktop candidate with that text only. The same code already checked *working-directory* identity (see `2026-11-06-organizer-working-directory-alias.md`), but not referenced **file** identity. A real local path spelled `Desktop\\.\\Image.png` or `\\\\?\\C:\\...\\Desktop\\.\\Image.png` can refer to the same file while failing text comparison. Therefore opt-in Analyze could offer the file and an older preview could still move it at Apply.

**Primary Windows contracts:** Microsoft documents 8.3/long-path aliases and that short names need not be enabled: https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file . A Windows handle's volume serial number and file index identify the opened file across path spellings: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information . These are filesystem contracts; they are not evidence of measured human productivity or a confirmed Desktop Pilot customer report.

## Fix and safety boundary

For **opt-in ordinary-file analysis only**, the production dependency collector resolves known saved launch-target and icon paths when the path is local, exists and its **parent directory identity is exactly the personal Desktop root**. Those file identities are protected in addition to pre-existing normalized-path and argument matching. The desktop candidate's already-inspected file ID is reused in both preview and Apply; an extra I/O operation per candidate is avoided. Repeated alias strings are memoized. The default shortcut-only preview does not add these probes.

Never access network or mapped drives to determine alias identity. Unrelated local directories are not accepted by file ID alone: an NTFS hard link in an external directory is not a dependency on the desktop directory entry, and moving the desktop name does not delete the external link. Existing shortcuts, user policies, folder positions, SQL history and Apply/Undo transaction safeguards are otherwise unchanged.

## Reproducible production sequence

`src/organize/consent_tests.rs::aliased_local_launch_or_icon_path_cannot_move_a_desktop_dependency` uses **the real organizer Analyze/Apply/Undo, real Windows file handles and NTFS hard links, and production SQLite** with two synthetic image files:

1. With no dependency, the opt-in baseline proposes two moves into one folder.
2. A known app referencing an external hard link to an image with the *same file ID* does **not** suppress that desktop image.
3. Change the saved app's launch target, then its icon path, to each deterministic alias of the Desktop file (ordinary dot-segment and extended local). Analyze must exclude the referred image while preserving independent candidates.
4. Apply the pre-change two-file plan after each new alias. The complete operation must be rejected before moving files, creating a folder, or writing a recovery journal.
5. Restore the app's external reference; a fresh, explicitly approved plan can move both images and Undo restores both, without changing their contents or the external link.

These are synthetic correctness counts (original plan: two eligible; after protection: one eligible, zero new-folder moves), not observed Windows 11 user interactions or measured time savings. Other limitations remain: unknown external applications, document-internal references, 8.3 availability, active cloud sync/OneDrive and physical Explorer icon placement. In particular, a genuinely unreadable or offline dependency is not inferred as safe from its filename alone.

## Acceptance

Run unchanged strict public Windows Server 2022/2025 CI: formatting, Rust tests, Clippy with warnings denied, build, native integration and release guards. Record exact run/commit separately after confirmation. This work targets the still-draft organizer; neither a green synthetic test nor this note clears live Windows 11, 100–200% GUI, multi-monitor, interruption recovery or OneDrive acceptance, and no organizer release is warranted.
