# Fresh organizer previews must show the complete selection — 2026-11-05

## Confirmed implementation behavior

The production organizer UI in `src/ui/organization.rs` retains `organization_query` when the user starts a new Analyze. The returned `Plan` selects recognized candidates by default, but the list is rendered through the retained search. Consequently a previous query that matches no current objects can display an empty list even though the global Apply action remains enabled and will move those selected objects. This is a source-confirmed UI inconsistency, not a reported Desktop Pilot user incident.

The global checkbox selection across filtered pages is deliberate; automatically dropping hidden selections would silently alter a user's earlier choices. The source code already shows a global move/folder/space summary, but previously did not explicitly explain that search is display-only.

## Independent context (not Desktop Pilot measurements)

- Microsoft Windows UI guidance, *Filtering collections*: filtering changes which list items are visible; it is a view on a larger source collection. https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/listview-filtering
- A first-party Fluent UI issue, *DetailsList and preserving selection* (#14278), records developer confusion around keeping checkbox selection through filtering. This is independent developer experience, not a Desktop Pilot user report: https://github.com/microsoft/fluentui/issues/14278
- The existing [adaptive-shortcuts research](2026-10-26-adaptive-shortcuts.md) discusses predictability and user control without claiming measured time savings from the organizer.

## Correction

- Explicitly clear the prior search and page when Analyze begins. The newly computed Plan is shown unfiltered; Apply still requires the user's click and unchanged filesystem validation.
- While a new filter is active, show a short RU/EN explanation that filtering hides only rows and Apply affects every checked object, including other pages. Do not change checkbox state, planned destinations, persisted Keep decisions, file contents, or history.
- Keep the same selection model and long-plan pagination. Do not add automatic file movement, telemetry or a new setting.

## Production-path regression and limits

`new_analysis_reveals_all_candidates_after_a_stale_filtered_preview` uses the actual `Settings` analysis job, the organizer's filesystem scanner, real Windows URL-shortcut fixtures and SQLite Store. After a complete two-item plan, it leaves an unmatched old project search and an out-of-range page, starts Analyze again, and verifies the complete two-selected-item plan is visible to the UI model, its page is reset, and files/Undo journal are unchanged. The old implementation fails the query-reset assertion.

**Before:** 0 of 2 planned items match the stale display filter, while the Apply action can still act on 2. **After:** a new analysis starts with 2 of 2 visible; any later explicit filter displays its global-selection scope. These are synthetic visibility counts, not human-time savings.

Existing Windows 2022/2025 strict CI, native filesystem/SQLite integration, GUI-layout tests and publication guards remain mandatory. Physical Windows 11, Explorer, live DPI scaling, cloud-redirection and interrupted-operation acceptance remain outstanding. This change stays in draft PR #98 and must not trigger a product release.
