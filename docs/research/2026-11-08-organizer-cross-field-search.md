# Searching a shortcut by its name and destination — 2026-11-08

## Confirmed gap, not a customer report

The unreleased organizer shows the source shortcut name next to its proposed destination. The production filter previously treated the complete text entered by a person as one substring and checked either field separately. A search such as `Sites Entry-` returned no results for a shortcut named `Entry-00.url` moving to `Sites`, although both terms were plainly visible on the same row. The same limitation applied when a user combined a skipped filename with the displayed reason it was protected. These are verified source behaviors, not evidence of customer complaints or time saved.

The prior [destination lookup regression](2026-11-07-organizer-destination-search.md) proves that each field is searchable independently; [filtered bulk review](2026-11-07-organizer-filtered-bulk.md) covers preserving selections on other pages. Neither previously covered terms spanning columns. Independent [Flow Launcher issue #2670](https://github.com/Flow-Launcher/Flow.Launcher/issues/2670) reports an expectation that directory names themselves should be searchable; it does **not** establish demand for this specific multi-column query. [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) discusses predictability alongside accuracy in adaptive interfaces. Consistent search over already shown fields is an application of that principle, not a result measured with Desktop Pilot users.

## Change and measurable fixture

Use case-insensitive, whitespace-separated **AND** terms: every word must occur somewhere in the visible name and destination (or the visible name and translated exclusion explanation). Order does not matter. Existing single-term matching is preserved. No fuzzy matching, new settings, hidden selection, network access or filesystem writes are introduced by the filter.

Extend existing production-path Windows tests using `native::read_link`, a real `Store`, the real organizer `Plan`, and clickable `Settings::organization_dialog` controls. Analyze 47 genuine temporary `.url` files, including 45 `Entry-` items. With `SITES entry-`, deselect exactly those 45 across two pages; preserve `Alpha` and `Beta`. Reverse the terms to reselect those 45. Mixed `entry- beta` must match none. Continue through the existing remember-kept path and verify files stay on disk, no category folder is created and no undo operation appears. The skipped-items fixture also checks the same mixed-field behavior in Russian and English.

**Before:** a valid two-field query had zero matches. **Expected after:** 45 correctly selectable entries; no unrelated selection changes. These are synthetic acceptance outcomes, not evidence of faster task completion in the field.

## Remaining acceptance

Validate the exact branch with cargo fmt, locked tests, strict Clippy, native Windows/Explorer/UI checks and build. Screenshots and physical usability checks, if available, remain separate. Do not publish the unreleased organizer `0.3.0` before its multi-monitor, keyboard-only, OneDrive and interruption acceptance gates. The stable `0.2.x` code is unaffected.
