# Temporary shortcut unavailability: conservative recovery (2026-10-26)

## Evidence, hypothesis and scope

- Findlater & McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004), studied 27 people and found user-adjustable menus generally preferred to automatically rearranging ones. https://doi.org/10.1145/985692.985704
- Todi et al., *Adapting User Interfaces with Model-based Reinforcement Learning* (2021), identify surprise and relearning as costs when changing familiar interface positions. https://arxiv.org/abs/2103.06807
- Hypothesis, **not measured use of Desktop Pilot**: removing a known working shortcut merely because a removable drive or synchronized source is briefly absent causes needless churn. The Windows filesystem does not prove whether a vanished shortcut is a temporary outage or a permanent uninstall.

## Confirmed behavior before the patch

`refresh_apps` immediately marked a missing source or unchanged link to an absent target `valid = false`. `model::select` then removed the published copy from selection. `engine::apply` removed the known-good owned copy on the next review even if the original/target would reappear later. Unlike sharing violations or an incomplete update (already protected), a missing file received no grace interval.

## Small, reversible change

- Persist `App.unavailable_since` (UTC seconds, optional/default-zero in old JSON rows). On the first confirmed disappearance of an *unchanged* source/target, keep it unavailable in ranking but protect **only an already owned** copy for up to **72 hours**.
- No new copies are ever placed from unavailable sources. The original source is revalidated for every newly added desktop copy, exactly as before.
- A verified different launch identity immediately invalidates the old copy; it is not covered by the grace interval.
- Explicit `Never` removes a managed copy without delay; reducing slot capacity can remove an unavailable copy. Manual rules and selected positions keep precedence.
- Once the source/target is valid again, clear the timestamp; expiry permits ordinary removal on the next review. The program does not run a new timer or secretly change the configured review schedule.
- The existing desktop copy, its file ID, bytes and history are never rewritten to simulate availability. No remote telemetry or API is involved.
- The Apps view continues to mark unavailable catalog entries and shows a local warning for a temporarily held owned copy.

## Reproducible scenarios

1. Begin with a genuine valid URL shortcut and publish a managed copy through `engine::apply`; remove its original .url file. Run apply again while within grace: copy bytes, position and file identity remain unchanged. Restore source and reapply: same ownership is preserved, warning clears.
2. Make a .url target point to an ordinary existing local file, remove target, apply, restore target, apply. Only validity and the saved unavailability timestamp change.
3. Remove one unchanged original and change a second URL identity: old changed-identity copy is immediately removed, absent source kept. Advance stored outage past 72 hours and apply: the remaining copy is removed normally.
4. Run `model::select` with an in-grace owned app and a stronger available challenger: the owned slot stays stable. The same unavailable app cannot be newly placed into an empty state.
5. Test expiry, an explicit `Never` rule, and an explicit reduction in slot capacity: all override the grace protection.

The fixtures call production selection, catalog scan, native shortcut parsing, and real create/remove/rollback paths with inert test files on Windows. They do **not** emulate a physical drive disconnect or quantify task-time savings. User-authored deletes of managed desktop copies are still handled by `reconcile_manual`.

## Validation

Run locked Rust formatting, tests, Clippy, build and native integration fixtures on Windows 2022/2025 in the repository CI. A green run is necessary before merging; physical Explorer, real removable storage and real human use require separate validation.

## Next scenarios

- Explore reconnect/disconnect exactly across 72-hour windows and system clock adjustments on a physical Windows machine.
- Quantify accumulated vacancy duration, spatial churn and false removals across long project switches on actual selection logic.
- Check graceful warning presentation at 150–200% UI scale and with long/non-Latin drive paths.
