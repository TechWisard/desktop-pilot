# Keep proposed folders understandable when an item stays — 2026-11-08

## Confirmed source behavior

The unreleased organizer shows a proposed destination only while `Plan::will_move` is true. Unchecking one of two site shortcuts removes the destination from **both** rows: the unchecked row stays by choice, and the checked singleton stays because creating a one-item folder would not free desktop space. Yet the same planned folder remains searchable through the actual `Plan::destination` field. Searching for `Sites` can therefore return rows that no longer display the searched term, making it difficult to understand what a bulk action will affect. This is a directly verified discrepancy between the production UI and plan logic, not a reported data-loss incident.

The previously implemented `2026-11-07-organizer-destination-search.md` and `2026-11-08-organizer-zero-move-guidance.md` cover the individual halves of this scenario but not their combination. Gajos et al., *Predictability and accuracy in adaptive user interfaces* (CHI 2008), https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml , motivates consistent action explanations. Independent Stardock Fences users have reported manual desktop arrangement surprises after monitor changes, e.g. https://forums.stardock.com/541585/desktop-icon-spacing-all-messed-up (2026-05-12). Those reports concern another application and **do not establish this defect or any measured time saving in Desktop Pilot**.

## Change

Keep the existing short moving-row label `Shortcut » Destination`. For a row that currently **will not move**, show `Shortcut — Stays in place · Planned folder: Destination` (with the Russian equivalent). The wording distinguishes a candidate category from an actual move and keeps the very field used for filtering visible, even when two selected items become one singleton. No new controls, automatic decisions, schema changes, telemetry, filesystem or policy mutations.

## Production regression

`kept_and_singleton_rows_keep_their_searchable_planned_folder_visible` uses two genuine temporary Windows `.url` shortcuts, `native::read_link`, SQLite `Store`, `organize::analyze`, `Plan::summary` / `Plan::destination`, and real egui paint output. It checks an initially moving row, then deselects one shortcut and checks both the unchecked row and the still-checked singleton in English and Russian. It also verifies that the displayed candidate folder is still the filterable folder, shortcut bytes are unchanged, no undo journal exists, and no job has started.

**Before:** the checked-to-singleton transition hides the searchable candidate destination on 2 of 2 rows. **After:** it is visible and explicitly marked as not moving on 2 of 2 rows, while actual filesystem moves stay at zero. Synthetic rendering and correctness checks do not measure user productivity. The existing 1×/2× screenshot and complete Windows CI checks must pass on the exact proposed commit.

## Completing native acceptance of the changed state

Continue the existing #246 history, reconciled with the independently merged multipart archive guard #247. Preserve both parents, stable main and all existing tests. The previously successful #246 run 42486718140 is historical evidence, not validation of the combined head.

The former screenshot driver only captured fully selected plans, filtered plans, optional files and an error. None showed the changed kept-row label. The `organization-kept` scenario now prepares a real analyzed plan with one selected site shortcut and two unchecked site shortcuts. All three stay on the desktop; the native UI must show both unchecked and selected-singleton labels, the candidate folder and one Remember action. The driver captures this state in both languages at 100% and 200%, including a bottom-scrolled 200% view. This adds six captures, not a new product setting or renderer. Its `test_singleton_review` value is read only after the existing explicit-capture and isolated-profile directory guards.

`src/ui/smoke/kept_tests.rs` checks the actual native shortcut parser and SQLite analyzer: ordinary capture still proposes all three moves; the singleton capture proposes none without changing catalog, an important Always rule, usage history, files or the connection's SQLite change count. A deficient one-item fixture must fail rather than silently produce an irrelevant screenshot. The existing real egui regression still checks the visible text and searchable destination.

The native screenshot driver now compares SHA-256 for all five fixture files before and after **every** scene, in addition to the existing directory membership/count and process checks. A screenshot is not accepted merely because the original filenames still exist. Test fixtures remain synthetic and do not open URLs or operate on a person's desktop.

Record final head, actual CI outcomes, screenshot review and artifact backup verification in the PR discussion. This document does not predeclare passing results. The proposed additional native scenes specifically address the missing visual evidence; they are not physical keyboard or hardware-DPI acceptance.

## Release boundary

Keep this correction on the unreleased 0.3.0 organizer draft until physical Windows 11 multi-monitor/DPI, active OneDrive redirection, keyboard accessibility and interrupted-power acceptance are independently verified. Do not promote this isolated rendering fix into a release by itself.
