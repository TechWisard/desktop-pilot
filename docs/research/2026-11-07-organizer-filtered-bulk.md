# Filtered bulk selection in the organizer preview — 2026-11-07

## Source observation and scenario

The unreleased organizer preview has a searchable, 40-item paginated plan and existing **Select all** / **Deselect all** controls that intentionally affect the complete plan, even when no rows match the search (production UI and `one_bulk_choice_changes_the_entire_real_plan_even_with_hidden_rows`). A user preparing a large mixed desktop cannot currently select or keep only the searched subset without checking each item on several pages. This is a source-confirmed missing workflow, not evidence of user dissatisfaction or measured time saved.

Microsoft's [File Explorer documentation](https://support.microsoft.com/en-us/accessibility/windows/use-a-screen-reader-to-explore-and-navigate-file-explorer-in-windows) describes search results and individual/multiple selection as distinct visible workflows. [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) found that accuracy and predictability both matter for adaptive UI usability. Neither publication tests this organizer; the expectation that a search-scoped bulk action reduces repetitive review is a **hypothesis** requiring user validation.

## Change and concrete regression

When search is nonempty, show **Select matches** and **Deselect matches** next to the number selected within the result set. These operate on every matching result, including other pages, and never alter hidden nonmatches. The familiar all-plan actions keep their existing behavior and visible notice. This is an explicit optional review action: no background change, telemetry, hidden file moves, or settings migration.

`filtered_bulk_review_keeps_unmatched_choices_across_organizer_pages` uses real Windows .url fixtures, an actual Store and the production `Settings::organization_dialog` widget click handler. It constructs 47 preview items, filters 45 across two pages, selectively clears/restores the subset, changes project focus, verifies unrelated choices remain, and exercises the existing global action followed by the real remember-kept operation. No files are moved. The test counts selection errors, unwanted changes (zero), and uses the same live `Plan` and `summary` logic as the app.

## Verification and release boundary

Run formatting, locked Rust tests, strict Clippy, native Explorer/cloud/UI fixtures and Windows builds on the exact branch commit. This draft is not in stable 0.2.126. Hosted fixtures do not replace physical Windows 11, keyboard-only interaction, DPI/topology transitions, OneDrive account integration, or controlled power interruption testing. Do not release 0.3.0 until those independent gates are satisfied.
