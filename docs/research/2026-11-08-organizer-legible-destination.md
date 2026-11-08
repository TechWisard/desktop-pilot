# Legible organizer destinations at 200% scaling — 2026-11-08

## Confirmed visual defect

The native Windows 11 ARM evidence from [Windows checks run 42448735931](https://github.com/TechWisard/desktop-pilot/actions/runs/42448735931), specifically `en-200-organization-bottom/dialog-organization.png` and `ru-200-organization-bottom/dialog-organization.png`, visibly rendered the arrow between a shortcut name and its destination folder as an empty square. The Windows UI screenshots were inspected, not inferred from code. The label source used Unicode U+2192; the default egui font did not provide a usable glyph in this environment.

A rendered unknown-character box makes a key action's destination ambiguous, especially at large UI scale. This is a directly observed Desktop Pilot defect, distinct from the user-reported Windows desktop icon reorder issues.

## Fix and checks

Use U+00BB (a standard Latin-1 right-pointing breadcrumb marker) between shortcut and folder. This retains the original one-line checkbox and destination and avoids introducing new fonts, licensing requirements, or layout controls. The production-path 45-shortcut egui scroll regression now checks the exact rendered checkbox label at 1× and 2× zoom alongside the fixed Apply control. Existing Windows Explorer and UI screenshot checks must still pass; inspect new screenshots to verify actual glyph coverage. A text assertion is not a raster glyph test.

No files, rules, placements, or settings change; this affects only the review label. No user time savings are claimed. The organizer remains draft pending physical Windows 11, multi-display, real OneDrive and interrupted-operation acceptance.
