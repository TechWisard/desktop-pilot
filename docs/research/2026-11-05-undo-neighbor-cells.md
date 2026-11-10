# Adjacent icon cells during organizer Undo — 2026-11-05

## Source observation and user benefit

The draft organizer records each original Explorer icon position before moving desktop shortcuts. On Undo, the production restoration code omitted an icon whenever another currently visible icon occupied its original grid cell. That check did not distinguish between a **stationary user icon** and **another original that the same Undo call is also moving**. After Explorer has reassigned two returning shortcut positions, the old code could reject both positions even though a single batch would move both originals back without touching unrelated icons. This is a code-grounded scenario, not a reported Desktop Pilot user incident.

Microsoft documents `IFolderView::SelectAndPositionItems` as accepting multiple child identifiers and corresponding positions in one request: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-selectandpositionitems. The project's native `position_many` already uses this API and verifies the actual result. It remains authoritative: a successful planner cannot guarantee Explorer honors every coordinate.

Unintended icon movement is a real user concern in other Windows layout tools, as documented in https://github.com/microsoft/PowerToys/issues/3143 and https://github.com/microsoft/PowerToys/issues/38682; those reports do not establish that this particular Desktop Pilot defect occurred to a person. Background research on predictable adaptive interfaces remains in `docs/research/2026-10-26-adaptive-shortcuts.md`.

## Actual production-path correction

`restore_layout` uses `restore_position_plan` to build a batch from the real `native::Layout` snapshot and actual Undo journal targets. A destination occupied by another candidate is permitted only if that candidate is also moving. If the other candidate cannot move because of a stationary or unrelated occupant, dependent moves are also rejected. Overlapping target cells are rejected, and manual icon positions outside the Undo operation are never selected. The existing native call and post-move coordinate verification are unchanged.

The spatial lookup indexes current and desired positions by grid neighborhood; blocked dependencies are propagated once. This avoids rescanning every desktop icon for every dependency. The journal, filesystem identity checks, collision protections, SQLite transaction, explicit Keep rules and background worker boundaries are unchanged.

## Regression scenarios and measurements

The `src/organize/journal.rs` tests invoke the production planner directly with synthetic `native::Layout` fixtures:

1. Two returning originals exchange temporary Explorer cells: before, 0 of 2 old positions would be requested; with the correction, 2 of 2 are requested in the same native batch and 0 are skipped.
2. Three restoration candidates form a dependency chain ending in an unrelated icon: all 3 are skipped, with 0 displacements of unrelated icons.
3. A free independent position is still restored when another target is blocked: 1 is requested, 1 is skipped.
4. Two requested positions overlap: both are rejected rather than creating a collision.

These are deterministic position-request counts on the same production planner, **not measured Explorer success, response time or user time saved**. Existing file identity, manual-return, Undo and SQLite tests remain part of Windows CI.

## Acceptance and remaining work

Run the unchanged strict Windows test/build pipeline on both Windows Server 2022 and 2025. This change remains in the organizer draft: do not merge or release without physical Windows 11 Explorer verification at 100% and 200% scaling, the arrangement after simultaneous return, OneDrive redirected desktop behavior, live interruption/undo and worker pause/resume. If Explorer repositions items unpredictably, retain its diagnostic warning and avoid touching stationary icons.
