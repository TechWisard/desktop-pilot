# Preserve manual icon placement during organizer Undo — 2026-11-05

## Observed source defect and user impact

The draft organizer already treats an original moved back by hand as an idempotent Undo item (verified filesystem identity). However, its layout restoration previously targeted **every** journal entry. On an Explorer desktop with positioning available, Undo could therefore move that manually returned icon back to its old grid cell, overriding the user's later decision. This is a source-derived defect and a plausible user workflow, **not** a confirmed real-user report. The earlier filesystem regression covers manually returned files but cannot check physical Explorer icon positions.

The published CHI 2008 experiment on predictability and accuracy in adaptive UIs describes the importance of predictable behavior and satisfaction, but does not measure Desktop Pilot:
https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml
Existing organizer evidence and acceptance boundaries: [desktop organization](2026-10-31-desktop-organization.md).

## Reproduction and change

Sequence: organize Alpha.url and Beta.url; user manually returns Alpha.url to the desktop and places it elsewhere; user requests Undo; the true Undo file-move path restores only Beta.url, but the old layout projection also targets Alpha.url. The replacement passes **only successful renames made by this Undo invocation** to the real layout restore routine. It leaves an already returned Alpha.url at the user's position. Apply still positions newly created folders; missing/occupied cells still return warnings; neither history nor user data is rewritten.

Reproducible `undo_preserves_the_position_of_an_original_manually_restored_first` exercises the same production position-target selection with two synthetic files, then an empty replay. Existing `manually_restored_original_is_accepted_during_batch_undo` exercises real Windows filesystem identity, SQLite and idempotent Undo. Together they check the target-selection rule and file recovery, **not** physical icon layout.

## Verification and remaining acceptance

Run the existing strict Windows CI on 2022 and 2025: all Rust tests, native integration, fmt, Clippy with warnings denied, build, release guards. Physical Explorer icon placement, scaling 100–200%, Windows 11, OneDrive redirection, crash timing, and actual manual drag positions still require live acceptance. This remains draft and is not a release.
