# Recovery acceptance must retain unverified originals — 2026-11-05

## Source-confirmed failure and practical impact

The explicit **Accept current locations** action for an interrupted desktop organization operation used a verified-file-ID check for each original at its desktop name or organized folder. However, if neither path contained the original (the file was moved elsewhere, deleted, or replaced), both branches were skipped and the transaction still removed the catalog-write barrier and marked the recovery journal **Accepted**. This could leave a stale application source path and make the unresolved original harder to recover. This is a source-level correctness defect, not a confirmed user incident.

An explicit user confirmation should not silently convert an unlocated original into a successful reconciliation. The existing journal, recovery status, app paths and user files must remain available for an informed retry. This is particularly important after process interruption and while the user manually moves files between folders.

## Relevant evidence, with limits

- Microsoft's file-identity contract requires using a file ID together with the volume serial to establish that two handles refer to the same object: https://learn.microsoft.com/en-us/windows/desktop/api/fileapi/ns-fileapi-by_handle_file_information
- The project already documents the same-volume identity-checked recovery mechanism in `docs/research/2026-10-31-desktop-organization.md`. It also contains a native regression for successful acceptance of a mixed partially moved state.
- Independent PowerToys user reports concern the importance of preserving manually arranged desktop content (https://github.com/microsoft/PowerToys/issues/3143 and https://github.com/microsoft/PowerToys/issues/38682). They do **not** document this Desktop Pilot failure.

## Change and reproducible sequence

The production acceptance transaction now **fails closed** when an expected original is not verified at either recorded location. SQLite rolls back earlier source-path changes and the attempted barrier removal; the operation remains pending, with the journal and user files intact. The explanatory error asks the user to restore that original to its recorded desktop filename or organized folder and retry. No automatic search, deletion, overwrite, telemetry or new setting was added.

`src/organize/tests.rs::accept_current_keeps_recovery_pending_for_missing_or_replaced_originals` exercises the actual Windows handle identity routines and SQLite Store in both cases:

1. Interrupt a two-shortcut move after the first rename, preserving its original filesystem ID.
2. Either move the still-unmoved second original outside the desktop, or replace the already-organized first original with foreign content.
3. Attempt Accept and assert that the journal stays Prepared, the barrier remains active and no catalog updates are committed; the original and replacement are preserved.
4. Restore the original without overwriting the foreign object; retry Accept and verify both app source paths, identities and cleared pending state.

This checks recovery correctness using synthetic files on real production logic, not users' measured productivity.

## Verification boundaries and follow-up

Use unchanged strict Windows CI (Rust tests, format, Clippy, build, native integration, release guards) for this branch and after any integration into draft PR #98. Physical Windows 11 Explorer positioning, 100–200% UI, OneDrive redirection, cloud sync timing, actual crash injection, and human user-session results remain unverified. Keep organizer draft; no release solely for this correction.
