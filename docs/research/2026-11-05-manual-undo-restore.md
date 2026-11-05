# Undo after an original was manually dragged back — 2026-11-05

## Scope and evidence

An owner can organize two web shortcuts into a category folder, then drag one original back onto the desktop before pressing Undo. The committed Undo preflight used to reject **every** occupied original name as a conflict. This included the *same original file* already returned by the user, although the later per-file Undo loop explicitly treats that file identity as already restored.

This mismatch is established by the production `journal::undo_locked` source. It is not a measured frequency of Desktop Pilot user failures. The [Microsoft MoveFileEx reference](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) documents Windows rename semantics and file permissions; the implementation uses the existing Windows file-ID checks rather than assuming equal bytes or names imply identity. A [November 2025 Stardock Fences discussion](https://forums.stardock.com/539431/fences-6-cant-restore) contains firsthand reports of manual restoration after failed layout recovery in a *different application*. That supports investigating the scenario, not a claim that Desktop Pilot exhibited it for real users.

## Reproduction and correction

`src/organize/acceptance_tests.rs::manually_restored_original_is_accepted_during_batch_undo` runs actual Windows filesystem renames, the SQLite-backed Store, the normal category-plan Apply and committed Undo:

1. Create two native-parsed `.url` originals, record IDs, file stamps, app sources, history and background-disabled mode.
2. Apply organization: both originals are in the new Sites folder.
3. Simulate the user dragging Alpha back to the desktop, preserving its filesystem identity.
4. Undo the operation. **Before:** preflight refused the occupied Alpha filename, leaving Beta in the folder. **Expected after:** Alpha is recognized as already restored, Beta is moved back, the journal becomes Undone, and both app paths are restored.
5. Verify identity, unchanged file stamps/history, no pending barrier and no background restart.

The preflight skips only a source whose file ID matches its journaled original. If the desktop name is occupied by a *different* file (including identical bytes), the existing collision error remains; the other items must not move. Missing/replaced organized objects or folders remain separately guarded. A user-authored edit to the same original is still permitted by the established Undo contract.

## Acceptance and limits

Synthetic outcome: two originals returned with zero overwritten foreign files; one already-correct original requires no move, the other requires one move. No real-user time reduction is inferred. Check unchanged Windows 2022/2025 CI with locked Rust tests, native filesystem tests, formatting, strict Clippy, build and release guards. Preserve the feature inside draft #98 until interactive Windows 11, DPI 100–200%, OneDrive-redirected desktop and interruption/recovery acceptance; do not publish as a production release.
