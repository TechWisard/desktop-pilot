# Visible excluded-only search — 2026-11-08

## Source observation and decision

The organizer presents a search over proposed shortcuts and excluded objects, including translated safety explanations. With an exclusion-only query, the proposed list is empty but the excluded list is initially collapsed. The user has to discover and open an additional disclosure to see the single matching object. With a query matching neither list, the previous message still suggested checking a left-in-place list, although that list does not render at all. These are observable consequences of `Settings::organization_dialog`, not reports of a field incident.

Existing [exclusion reason search](2026-11-08-organizer-skip-reason-search.md), [empty search recovery](2026-11-08-organizer-empty-search.md) and [cross-field search](2026-11-08-organizer-cross-field-search.md) scenarios establish the behavior without duplicating an external research sweep. Gajos et al., *Predictability and accuracy in adaptive user interfaces* (CHI 2008), https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml , motivates predictable presentation as a general design concern; it does not establish measured benefits for this screen.

## Narrow change

For a nonempty search, the excluded-items disclosure defaults open on its initial appearance. It still respects a later manual collapse. An all-empty query now says no item in this review matched, instead of referring to a nonexistent excluded-results section. An excluded-only query explicitly points to those matching rows below. Query changes and clear/search actions remain read-only: no preferences, profiles, files, selections, undo journals or tracking state are modified.

## Acceptance and limits

The new production-egui test `searching_only_excluded_objects_reveals_matches_and_distinguishes_no_results` loads actual Windows `.url` entries through the native shortcut parser into SQLite, remembers one explicit Never policy, analyzes the live temporary desktop, and paints English/Russian dialogs. It checks a matching excluded row is visible without an extra disclosure click, a completely unmatched query offers truthful recovery, and saved policy, source bytes, selection and Undo remain unchanged. Check the exact proposed commit using the mandatory format, locked tests, strict Clippy, build and native Windows UI/Explorer gates. This is a correctness assertion using synthetic fixtures, not a measured reduction in human task time.

The 0.3.0 organizer stays draft pending physical multi-display Windows 11, active OneDrive, keyboard and interrupted-power acceptance. No stable release is implied.
