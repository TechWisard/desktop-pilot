# Fresh excluded-results page on desktop reanalysis — 2026-11-08

## Existing observation

The organizer uses a separate egui temporary page cursor for objects deliberately left in place (`organize-skipped-page`). The normal Analyze action already clears the proposed-item search and its page, but it did not clear this other cursor. With more than 40 excluded objects, a person returning from another project can receive a *new* analysis while the excluded-items section still opens on page two from the earlier review. The old cursor is not a saved user rule. Source check: `Settings::organization_dialog`, `organization_page_bounds` and `Settings::organization_analyze`.

This is a reproducible UI-state defect, not a reported production data loss. The research already collected for the organizer references Gajos et al., *Predictability and accuracy in adaptive user interfaces* (CHI 2008), https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml . Microsoft Windows navigation guidance also identifies consistency and clarity as design principles: https://learn.microsoft.com/windows/apps/design/basics/navigation-basics . Neither source evaluates this exact pagination screen; transferring the principle here is a design hypothesis.

## Small correction

When the user clicks **Analyze the desktop** in the real dialog, reset the excluded-results page to the first page, alongside the existing main-page and query reset. Do not clear exclusions, remembered manual decisions or other file state. Navigation while browsing an existing review is unchanged.

## Production-path regression

`reanalyzing_from_the_dialog_resets_the_old_excluded_results_page` creates 44 genuine `.url` shortcuts on an isolated test desktop, uses the real asynchronous analyzer and SQLite keep-only operation, and establishes over one page of exclusions. It then retains the same egui context with the old excluded-page cursor set to two, clicks **Analyze the desktop** using the actual production control, and checks that both page cursors and the search return to the beginning without changing saved exclusions, original file bytes or the Undo journal.

**Synthetic before/after:** stale excluded-page cursor after new analysis: **1 → 0** (zero-based). Files moved: **0 → 0**. There is no claim of measured time saving for real people. The existing Windows, Explorer, UI, cloud and interruption gates still apply to the unreleased organizer.

## Follow-up

Check this exact commit in public Windows CI (format, Rust tests, Clippy, build and native integration). Complete physical Windows 11 multi-monitor, active OneDrive and interrupted-power acceptance before releasing the organizer.
