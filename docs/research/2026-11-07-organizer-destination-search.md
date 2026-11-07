# Organizer search by visible destination — 2026-11-07

## Observed source behavior and user task

The draft organizer displays a destination folder beside every proposed move, including the `Sites` folder for ordinary web shortcuts, and supports a search-scoped **Select matches / Deselect matches** action over all pages. Inspection of the production filter found that only the original filename was searchable. A person who sees 47 shortcuts destined for `Sites` and searches for the displayed group name gets zero matches; the button cannot select or keep that category together. This mismatch was verified from the UI code, not a claim from an external Desktop Pilot user report.

The existing [predictability/accuracy background](2026-10-26-adaptive-shortcuts.md) and [filtered bulk review](2026-11-07-organizer-filtered-bulk.md) motivate consistency between the text the interface shows and what can be filtered. Reducing repetitive selection is a hypothesis, not a measured human time saving.

## Narrow change and production regression

Match the visible destination folder as well as the item name, case-insensitively, using the actual `Plan::destination` result. No new modes, controls, implicit action, persistence schema, changed launch identity or telemetry. A skipped item without a destination still uses its name. Display the search label as **Search by name or folder / Поиск по имени или папке**.

`searching_destination_folders_bulk_reviews_every_matching_page` loads 47 genuine temporary Windows `.url` fixtures through `native::read_link` and `Store`, analyzes the real plan, and clicks the actual egui modal bulk-action controls. Search for `Sites` must affect all 47 objects over two pages; a subsequent `Alpha` search affects only that object. The test restores all selections, deselects the category again, persists keep-on-desktop choices, reanalyzes, and asserts zero moved files and zero undo journal.

**Before:** no `Sites` matches and the bulk action is ineffective. **After:** 47 category matches with no unintended file moves. Synthetic fixture correctness is not field evidence of productivity.

## Acceptance boundaries

Run mandatory Windows formatting, locked Rust tests, strict Clippy, native Explorer/UI/cloud checks and build on the exact commit. Keep this on the unreleased 0.3.0 draft. Real two-display Windows 11, OneDrive account and interrupted-power acceptance are still required independently; no release is justified by this one scenario.
