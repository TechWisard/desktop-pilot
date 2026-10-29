# Recover after a successful rename followed by attribute-setting failure (2026-10-30)

## Evidence and scope

Starting point: Desktop Pilot 0.2.59 at `7cd3fc97dc72684e384784070ed2b44437f9b7e9`, successful main/release Windows Actions 40953222854, no open PR. This continues the existing reversible-shortcut and ownership work; it does not repeat the broader adaptive-interface search. The normal public repository and its Windows runners are sufficient; no secondary test repository or paid service is necessary.

Microsoft's [MoveFileExW documentation](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) describes completion of the move, while [SetFileAttributesW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileattributesw) is a distinct fallible operation. A new shortcut is staged with the HIDDEN attribute and renamed before Desktop Pilot clears that attribute. Source review identified a partially completed failure path: if the rename succeeds and the attribute update then fails, `move_new` returns an error, yet `copy_new` only removes the now-nonexistent temporary pathname. The hidden final file is not registered as owned or reliably undone. This is a source-confirmed correctness boundary, not a user-reported frequency or measured rate.

## Failure-first real production path

`engine::tests::failed_attribute_update_after_rename_must_not_leave_a_hidden_unowned_copy` uses the real `copy_new` / `move_new` on a Windows test profile, original inert URL bytes, and the actual filesystem rename. One test-only, thread-local failure injection returns an error **after** successful `MoveFileExW` but before the attribute update, corresponding to a Win32 failure at that boundary. It then requires no leftover destination, no temporary file, an untouched source, and successful retry.

The first test commit was rejected by `cargo fmt --check`; it is not behavioral evidence. After formatting, [failure-first Windows CI 40955622898](https://github.com/TechWisard/desktop-pilot/actions/runs/40955622898) executed the real test on Windows 2022 and Windows 2025. Each runner recorded 306 passing Rust tests and one failing new regression because the unowned destination remained. This is deterministic injected error evidence, not proof of an observed real-world ACL failure.

## Correction and safety boundary

Capture the staged file's optional stable filesystem identity after writing and flushing, before closing its handle. On a failed publish operation, clean the destination **only if** the staged pathname disappeared and the candidate destination still matches both the staged identity and exact content hash. Reuse the existing guarded `remove_owned` path, which refuses a replaced/edited file. On a missing stable identity or refused removal, return a conspicuous error with the destination path rather than deleting an unverified file. Normal successful publishes and pre-rename collisions retain their previous behavior. No new watcher, schema, persistent data, dependency, telemetry, settings or background work.

## Required checks and remaining acceptance

Execute unchanged Windows Server 2022/2025 `cargo fmt --check`, locked Rust tests, strict Clippy, locked build, 29 native integration checks, nine release guards and README validation. For a release, separately run the Windows x64 release-native and packaged asset checks. Distinguish checked-in test results from actual physical Windows 11 human-use or network-drive measurements. The existing broader cold-start, real network outage, long-duration changes and 200% DPI tasks remain open in issue #22.
