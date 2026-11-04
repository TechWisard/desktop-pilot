# Keep-only decisions must not interrupt usage observation — 2026-11-04

## Evidence and user impact

The existing organizer draft already lets a person reject all proposed desktop moves and remember that choice without modifying a shortcut. However, the actual Settings job still called `setup::with_organization_paused` for every Apply. That function acquires the startup lock, stops the background process if running, and subsequently restarts it. Keeping files in their current positions needs no filesystem mutation and no background interruption. The mismatch is directly established by the previous `Settings::organization_apply` and `apply_stopped` paths; it is not inferred from third-party reports.

Existing research on spatial consistency and predictable adaptive UIs is documented in [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md) and [zero replacements](2026-11-04-zero-replacements.md). The **new hypothesis**, not a user-measured fact, is that saving a negative organization choice should be immediate and non-disruptive even while a user is actively working.

## Change and real-program reproduction

- Keep-only consent now uses a dedicated `remember_keep` path that takes the same **changes** and **organization** locks as moving operations, verifies the current desktop identity and the still-unchanged rejected files, checks pending placement/organization recovery, and stores exclusions in one SQLite transaction. It cannot accept a plan containing actual moves.
- The normal file-moving operation continues using the original worker pause, startup serialization, durable journal, Undo and recovery behavior. No lock, validation or release requirement is removed from that path.
- The real Settings job detects whether the production plan has any movements; the dedicated path is used only for zero movements. The modal reports “Saving keep choices” rather than implying that files are being moved.
- The new regression holds the actual background singleton mutex during an asynchronous Settings keep-only job and confirms that the job completes before the guard is released, with both original `.url` files unchanged, no shutdown intent, no fabricated Undo journal, and choices still excluded after re-analysis. A separate test proves that the keep-only entry point refuses a plan containing moves.
- Existing tests for changed files after preview, persistent consent, mixed Move/Keep, worker pause and interrupted Undo remain required.

This is synthetic production-logic verification using Windows filesystem and SQLite fixtures. It does not claim an observed reduction in real users' time, and it is not a physical Windows 11 UI/OneDrive acceptance test.

## Release boundary

Keep this change isolated from the published `main` branch and from the ongoing release PR. The encompassing organizer feature remains draft pending live Windows 11 interaction, 100–200% DPI, redirected/OneDrive desktop handling, worker shutdown/resume and interruption recovery. Require both standard Windows CI runners (format, Rust tests, strict Clippy, build, native and publication guards) before incorporating the change into the organizer draft.
