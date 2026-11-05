# Mixed Move/Keep preview must recheck every decision (2026-11-05)

## Evidence and user impact

**Source-confirmed defect in draft PR #98, not a field-reported incident.** The organizer's Keep-only path checks each unchecked item's current file identity and shortcut bytes before saving an exclusion. In contrast, mixed Apply checked only the *selected* moves before relocating them. It subsequently added the unchecked items' **preview-time file IDs** to `organization.keep.v1`, even when those objects had been removed, edited, or replaced. Thus a person could approve moving Alpha/Beta while keeping Gamma, replace Gamma before Apply, and still get two file moves plus a stale Keep rule for the *old* Gamma. The new Gamma would be proposed again on future scans.

The existing research synthesis in [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md), particularly [Findlater and McGrenere (CHI 2004)](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) and [Gajos et al. (CHI 2008)](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), motivates respecting explicit user choices and minimizing surprising changes. These studies do not measure Desktop Pilot's actual performance or the frequency of this defect. The evidence for the defect is the application's previous Apply/Keep control flow.

## Change

Use one production `recheck_rejected(root, plan)` helper in **both** Keep-only and mixed Move/Keep operations. Under the existing filesystem and changes locks, re-inspect every unchecked object before creating any category directory, moving any file, installing the SQLite recovery barrier, or persisting any exclusion. Check filename, filesystem identity, attributes, size, modification time, and the full shortcut digest when applicable. A missing, replaced, or edited object invalidates the entire preview and requests re-analysis. Leave previously saved exclusions untouched on failure.

This does not relax the existing selected-move validation, modify user files, change the automatic planner, pause the worker for Keep-only decisions, or introduce telemetry. A fresh explicit preview can still move the chosen items while retaining the current unchecked objects.

## Production-path regression

`src/organize/consent_tests.rs::mixed_apply_rechecks_every_rejected_object_before_moving` uses the real Windows filesystem organizer, .url parsing and SQLite Store:

1. Analyze three local shortcuts and uncheck Gamma; the plan contains two moves into one new folder and one Keep decision.
2. Between Preview and Apply, independently try **missing**, **replaced with a different file ID**, and **edited in place** Gamma.
3. Require Apply to fail **before moving Alpha/Beta**, creating the Sites folder, changing the catalog, saving a Keep preference, or installing a pending recovery journal.
4. Reanalyze the changed (or manually restored) desktop, make the same deliberate selection, Apply and Undo successfully; ensure Alpha/Beta return intact and the current Gamma remains excluded.

The test intentionally exercises the actual Apply and Undo methods rather than a separate decision model. A failing test on the old branch is predicted from source inspection but was **not** executed against the old commit. The new branch must pass the normal Windows 2022/2025 test, strict Clippy, format, build, native integration and release guards before merging into draft #98.

## Release boundary and remaining checks

Keep this change in the organizer draft until live Windows 11 acceptance covers the RU/EN modal at 100–200% scaling, redirected/OneDrive desktop roots, multi-monitor desktop changes, worker restart, and interrupted operation recovery. Synthetic success does not establish human time saved or comprehensive reliability on a physical desktop.
