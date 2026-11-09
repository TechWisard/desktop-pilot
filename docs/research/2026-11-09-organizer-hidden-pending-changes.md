# Disclose hidden pending decisions in filtered organization — 2026-11-09

## Practical question

When a desktop review has a search filter, does the fixed confirmation show *everything that applying the full plan will change*, rather than only the checked rows the filter hides?

The unreleased organizer already provides two separate bulk scopes (whole plan and matched rows) and an explicit fixed-header count of checked rows outside search. The search never changes a selection. Actual Apply operates on the whole plan: it moves selected items in qualifying groups and remembers **all unchecked candidates** as Keep choices. A checked singleton in a new group cannot move. These are verified in the production `Plan::will_move`, `Plan::summary`, `Settings::organization_dialog` and `organize::remember_keep`/Apply paths, with existing real Windows/SQLite regression coverage. This is not a reported loss of customer data.

The old header counted only `selected && !matches_search`. A person could uncheck an item hidden by a later search, leave one checked singleton, and receive **no scope warning** even though clicking Remember will persist that hidden Keep choice. Conversely, the header could count a hidden checked singleton that will not move or be remembered. This mismatch is reproducible entirely through the real plan and egui renderer.

## External evidence and inference boundary

Gajos et al., *Predictability and accuracy in adaptive user interfaces*, CHI 2008, https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml , report that adaptation accuracy and predictability affect satisfaction and use. Their results do not specify a preferred desktop review label, count or layout.

Independent Stardock Fences users describe unexpected icon disappearance and the recovery burden: https://forums.stardock.com/504294/lost-all-icons-on-desktop-and-all-fences-disappeared (2021). A 2024 report describes icons rearranged across desktop refreshes: https://forums.stardock.com/528482/icons-in-fences-get-rearranged-when-desktop-is-rebooted-or-refreshed . Those are reports about a different application; they motivate transparent scope and recoverability but **do not demonstrate Desktop Pilot defects or actual productivity improvements**.

## Narrow correction

Replace the checked-outside-search indicator with **Changes outside search / Изменений вне поиска**, counting each filtered-out candidate exactly once if it will actually move **or** if it is unchecked and will be remembered as Keep. Checked new-folder singletons do not count. Excluded/protected objects also do not count, since Apply does not change them. A normal, unfiltered review needs no extra line. No new setting, automatic decision, file operation, telemetry or policy override.

`Plan::will_move` scans only until a matching second checked item. A naive per-item call in a 6000-row repaint would be unnecessarily expensive; cache the result by at most seven `Group` values during this one frame. The same explicit production plan remains the decision source, and actual Apply rechecks the filesystem.

## Reproduction and acceptance

The existing real Windows `.url` / SQLite / egui test has been expanded into `filtered_review_discloses_pending_moves_and_keep_choices_outside_search`, for RU and EN, at 100% and 200% zoom.

| Filter and choice | Before | Intended after |
| --- | --- | --- |
| Alpha visible; Alpha/Beta both selected and moving | Selected outside: 1 | Changes outside: 1 |
| Alpha visible; Beta unchecked, Alpha singleton | No warning although Beta Keep will persist | Changes outside: 1 |
| Alpha visible; both unchecked | No warning although Beta Keep will persist | Changes outside: 1 |
| Alpha visible; Beta checked singleton, Alpha unchecked | Selected outside: 1 although Beta will not move | No hidden-change warning |
| Beta visible; Alpha unchecked | No warning although Alpha Keep will persist | Changes outside: 1 |
| Beta visible; both moving | Selected outside: 1 | Changes outside: 1 |
| No filter | No warning | No warning |

The fixture also asserts that source bytes, operation journal and background-job state remain unchanged during review. These are synthetic correctness checks, not human time-saving measurements. Strict Windows 2022/2025/11 ARM and native screenshot acceptance are required before integration; no release should be inferred from a single green unit test.

## Safety and release boundary

Leave stable `main` and all 0.3.0 physical acceptance gates unchanged. Real multi-monitor topology changes, an active OneDrive account, keyboard/accessibility, interrupted power and hardware GPU rendering are not established by hosted fixtures. The separate ongoing sustained-work ranking PR must be evaluated independently; this fix changes only filtered review feedback.
