# Legacy managed shortcuts with alternate Desktop paths (2026-11-06)

## Evidence and user impact

Source inspection of draft organizer head `1b2e104462adeb1c9543bb0d1351692d0a37fd8a` found that `plan::analyze` and `journal::apply_stopped` exclude saved managed quick shortcuts by a normalized path string or an optional stable filesystem ID. Older serialized `State.owned` rows may omit `file_id`, and a Windows 8.3 or dot-segment spelling of the same Personal Desktop file does not necessarily compare equal to its canonical scanned path. In this combination, an already managed quick shortcut could be proposed for a second move into a category folder, disrupting the user's chosen placement. The risk is derived from the code; no actual customer incident has been verified.

Human-interface context: Gajos et al., CHI 2008, found that accuracy and predictability affected satisfaction and use in adaptive UIs (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). Findlater and Gajos (2009) emphasize evaluating task and user variability rather than a single algorithm metric (https://www.eecs.harvard.edu/~kgajos/papers/2009/findlater09design.shtml). An October 2025 Windows 11 user report describes icons moving unexpectedly during rename (https://www.reddit.com/r/WindowsHelp/comments/1oeq3n0/); this is an anecdote about Windows, **not evidence** that Desktop Pilot caused the behavior.

## Narrow fix

For legacy managed entries without a saved file ID whose path differs from the canonical Personal Desktop spelling, resolve a **local, non-network path** and verify that its parent directory is the *same physical Personal Desktop directory* by filesystem identity. Only then record the managed object's file ID. Exclude that object in both a newly built plan and the locked Apply preflight. Existing entries with IDs and exact paths remain on the fast path. Another hard link with the same ID **outside** the Personal Desktop must not protect a different desktop icon. No automatic rearrangement, change of user policy, telemetry or migration of persisted rows is introduced.

## Reproducible full-path scenario

`src/organize/consent_tests.rs::legacy_managed_alias_cannot_be_organized_even_from_a_stale_preview` uses the production `Store`, actual `.url` reader, real Windows filesystem identity and `analyze → apply → undo`:

1. Preview three independently organizable website shortcuts.
2. Simulate a concurrent restoration of a legacy managed placement under an alternate path without `file_id`. A stale Apply must reject the entire operation before any file/folder move, history update or catalog rewrite.
3. Reanalyze: one managed icon stays; two unrelated icons can still be moved and undone, with stored `State.owned` preserved.
4. Change the owned path to a verified hard link in a different directory: the unrelated Desktop sibling remains eligible.

Acceptance metrics: zero unintended managed moves, zero partial stale-Apply mutations, two valid unrelated moves and restoration, unchanged user-owned metadata. These are synthetic regression criteria, **not** measured real-user time savings or proof of Windows 11 Explorer layout behavior.

## Verification gates

Run strict existing Windows Server 2022/2025 tests, cargo formatting, Clippy, native integration, build and publication checks. Keep this change in a separate candidate PR against draft #98; do not publish the organizer until interactive Windows 11, 100–200% DPI, multiple monitors, OneDrive and interrupted recovery acceptance have actually been performed. The pre-existing release remains unchanged.
