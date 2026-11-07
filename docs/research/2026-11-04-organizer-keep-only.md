# Saving keep-on-desktop choices without moving files — 2026-11-04

## Evidence and scope

**Source-confirmed behavior in the draft desktop organizer (PR #98), not a measured field incident.** The preview allows a person to uncheck proposed moves, but `Apply selected moves` was disabled when all selected objects were unchecked. The only place writing those exclusions to `organization.keep.v1` was a successful move transaction. Rejecting every suggested move therefore could not be remembered: a later analysis proposed the same objects again. A user had to authorize an unrelated move merely to preserve a negative choice. This is especially disruptive at first run, when retaining an existing layout is a legitimate preference.

Previously accumulated research on predictable, adaptable interfaces and spatial consistency: [Findlater and McGrenere, CHI 2004](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) and [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). These studies motivate control, not a measured time saving for this program. The scenario is a source-confirmed usability gap; user prevalence is unknown.

## Change

- When at least one item was explicitly unchecked and no actual file move remains, the same Apply path saves only the selected exclusions. The UI calls this **Remember keep choices**, rather than offering a disabled move button.
- Under the existing changes and organization locks, recheck the current desktop root identity and every excluded file's name, attributes, filesystem identity, size, modification timestamp and shortcut digest. Refuse stale previews before writing any exclusion.
- Commit the exclusion set atomically to SQLite; preserve existing exclusions, avoid any move, folder creation, change journal or Undo entry. A future explicit reset restores the suggestions.
- The normal move path still remembers exclusions after successful placement; no behavior or permission change for moving actual files. No background organizing, telemetry or new settings.

## Reproducible acceptance

1. Analyze two verified local website shortcuts. Uncheck both; summary has zero moves. Save choices and verify the original bytes, original catalog, desktop paths, and journal/history stay unchanged, with exactly two stored exclusions.
2. Reopen the SQLite Store and analyze again: no suggestion reappears. Explicitly reset exclusions: both suggestions reappear.
3. Between preview and Save, edit one unchecked shortcut. Save must fail with zero new persisted exclusions, no renamed files, and no Undo journal.
4. Run the actual Settings job path with collection disabled. Saving exclusions must display a clear success message, leave the background worker disabled, and not require a move or a recovery operation.

## Verification and remaining acceptance

Require the existing Windows Server 2022 and 2025 pipeline: formatting, all Rust tests, strict Clippy, build, native integration and release guards. The change is confined to the **unfinished organizer draft**, not to released 0.2.112. Passing isolated Windows tests does not demonstrate real Windows 11 GUI scaling, OneDrive redirection, touch input, or actual multi-day user behavior. Preserve the draft status and do not publish a product release until those acceptance checks complete.
