# Do not warn about shortcut placement when the plan moves nothing — 2026-11-08

## Real scenario and source verification

A person unchecks one of two desktop site shortcuts. The remaining selected singleton also cannot move because a new folder requires two items. The production plan reports `(moves, folders, freed) = (0, 0, 0)`, and the header offers only **Remember keep choices**. However, the same render path still evaluates `organization_projection_available` and paints an amber **quick-shortcut area not confirmed** warning when no layout is available or no free zone fits. This warning asks for a layout refresh *after organizing*, although no organization will occur. It also runs the read-only projection despite there being no proposed change.

This is a verified source-level mismatch, not evidence of file corruption or measured task delay. The earlier `2026-11-08-organizer-zero-move-guidance.md` already established why singletons remain. The exact `organization-kept` Windows screenshot scene from #246 demonstrates the zero-move state; its preserved test manifests and image bytes remain in the existing private release-verification backup.

Independent context: Microsoft Community Hub participants report desktop icon position instability after Windows updates, especially with an external monitor (June 25–July 2, 2025, https://techcommunity.microsoft.com/discussions/windowsinsiderprogram/bugdesktop-icon-order-and-file-explorer-window-size-not-saved/4427257). This does **not** establish a Desktop Pilot defect. Gajos et al. (CHI 2008), *Predictability and accuracy in adaptive user interfaces*, https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml, motivates explanations consistent with actual actions, not an unverified productivity claim.

## Narrow fix and production-path regression

Render the warning only when `moves > 0`. Keep the original warning and its exhaustive zone calculation for any genuine move. Do not suppress Apply's live revalidation or change selections, rules, background operation, file access, screen layout, undo or release settings.

`keep_only_review_hides_irrelevant_quick_area_warning_but_preserves_real_warnings` creates real disposable Windows `.url` shortcuts, reads them into SQLite, analyzes with the production organizer and renders the real egui dialog in both languages. Sequence: two moves with the warning; deselect one and confirm zero moves, visible explanation, Remember control, no warning and no projection work; reselect and confirm that the warning returns. Check original shortcut bytes, catalog, journal and background-job state.

**Before (verified from source):** an unrelated warning appears in the zero-move branch; projection can run. **After (expected/tested by the regression):** no such warning in a keep-only preview, with the genuine-move warning retained. This is one fewer irrelevant alert, **not** a claim of measured human time saving.

## Release boundary

The fix belongs to the unreleased 0.3.0 organizer integration branch, not released stable main. Physical Windows 11 multi-display/DPI transitions, active-account OneDrive, keyboard/accessibility testing and real power interruption are still required for the complete organizer release. Keep these gates unchanged.
