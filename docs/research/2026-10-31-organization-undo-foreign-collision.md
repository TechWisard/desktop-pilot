# Organization Undo: foreign name collision before recovery

Date: 2026-10-31. Continuation of PR #98; not shipped.

## Practical scenario and evidence

An explicit organization moves two site shortcuts from the desktop into a category folder. Later the user creates a *different* file under the vacated first shortcut name. If Undo processes the other shortcut first, then encounters the occupied name, the old code entered the durable recovery phase and had already moved one original. The new file was not overwritten, but the user's request could leave a partly restored desktop and block other actions until recovery. This is a deterministic consequence of the old `undo_locked` ordering, not a claim about measured user frequency.

The research on adaptive interaction emphasizes user control and predictable response to consequential changes. Findings from a 27-person menu study are relevant design context but do **not** measure Desktop Pilot use:
https://doi.org/10.1145/985692.985704

## Change and regression

When the journal is still **Committed**, inspect *all* original names before installing the Undo barrier or moving anything. Verified managed quick-copy collisions retain their more specific message; all other occupied names receive an explicit localized instruction to move/rename the newer item, with an assurance that no file changed. A previously **Undoing/Prepared** journal keeps its existing recovery behavior, and the existing identity-checked per-file operations remain mandatory (the preflight does not remove TOCTOU protections).

`src/organize/acceptance_tests.rs::foreign_name_collision_rejects_undo_before_partial_moves_and_allows_retry` uses actual `Store`, real Windows Internet shortcuts and the normal handle-based filesystem moves. It creates a foreign replacement at Alpha, requires Beta to stay organized, checks both original identities, the new file's bytes, the unchanged SQLite catalog/history and a non-pending journal. It then renames the foreign file and successfully retries Undo, preserving all identities and contents.

This is a Windows fixture, not a simulation of real-user time savings. After CI, physical Windows 11 UI review and interruption/race acceptance remain required by PR #98. No release is authorized by this patch.
