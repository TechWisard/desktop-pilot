# Recovering from an empty organizer search — 2026-11-08

## Observed source behavior

The unreleased organizer keeps selections across the whole plan while its search narrows visible rows. A query with no proposed-item matches currently displays only the numeric 0/0 page range; the user must infer that objects still exist and manually erase the query. The separate left-in-place reasons are intentionally collapsible. This is confirmed from the existing production UI code and the real temporary Windows Internet Shortcut/SQLite fixture, not from a measured usability study.

Existing research already recorded in this repository: Findlater & McGrenere (CHI 2004), https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/ , and Gajos et al. (CHI 2008), https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml . Making a zero-result state explicit and its recovery reversible is a design hypothesis based on the general importance of predictable controls; neither study tests this exact screen.

## Change

Only while searching, offer a **Clear search** action that restores the full view and resets both page counters without modifying selections or user rules. If none of the *proposed items* match, say so explicitly and direct attention to the separate left-in-place list. Matching excluded objects may still exist, so do not say that no desktop item exists.

## Production-path regression

`empty_organizer_search_is_explained_and_can_be_cleared_without_losing_choices` drives the real Settings dialog and click handler using actual .url files from the Windows parser and the SQLite store. It verifies bilingual feedback, the one-click recovery, preserved mixed selections, unchanged source bytes, no remembered keep policy and no Undo journal. This is a synthetic correctness test, not a demonstrated human time saving.

The organizer remains unreleased. Its existing native Explorer, cloud, recovery and high-DPI CI checks plus outstanding physical Windows 11/OneDrive/multi-monitor acceptance are unchanged.

## Native regression caught during integration

The first patch's separate Clear button and empty-state text added enough vertical space to push the existing visible excluded-item heading below a 1400×1000 Windows review window. The production egui regression `searching_exclusion_reasons_finds_saved_manual_rules_across_pages_and_locales` caught this in Russian (533 other Rust tests passed on Windows 2022; strict formatter also reported three new assertion layouts). This was a **real UI regression**, not an upstream network outage. The revised control shares the search-label row and the no-match message replaces, rather than supplements, the existing filtered-result explanation. The existing exclusion search test remains mandatory and unmodified. No release is authorized until full exact-head CI passes.
