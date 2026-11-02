# Predictable and strictly bounded shortcut discovery — 2026-11-02

## Evidence and practical scenario

Continue the existing [large discovery](2026-10-28-large-discovery-limit.md) and [catalog stability](2026-10-27-noop-catalog-rescan.md) research rather than repeating literature searches. Microsoft's [FindNextFileW documentation](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findnextfilew) explicitly says directory enumeration order is *not guaranteed* and depends on the filesystem. A filesystem may return entries in creation order rather than alphabetical order. That is a documented OS property, **not** a measured user complaint about Desktop Pilot. Earlier [CHI research on adaptive interface predictability](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivates stable outcomes but does not evaluate this specific program.

**Source-confirmed defect:** the production recursive visitor checked its 6,000-shortcut budget only on entry to each recursive call and after pushing a matching file. A child could reach the limit and return to a parent, which would continue examining sibling entries. An eligible sibling file in the parent could be pushed past the global budget. Further parent levels could do the same. Also, sorting collected paths **after** the cap was reached did not stabilize *which* entries were retained when filesystem enumeration order changed. A multi-folder catalog near capacity can thus waste scanning work and produce inconsistent subsets; actual user incidence is unknown.

## Correction

Extract the same production visitor into a module-level function with the limit passed as an argument for small, reproducible tests. Stop each parent immediately when any child exhausts the shared count; apply the same budget across desktop/start-menu roots. Sort the entries within each directory by name **before** the cap is applied. Preserve the previous folder exclusions, reparse-point checks, extension filter, per-folder errors, category precedence, saved catalog rows, manual choices and warning on truncation. No telemetry, network access, new setting or user-file mutation.

The deterministic traversal is depth-first (not a global alphabetical ordering of all sources). The desktop root still has precedence over additional roots. A directory with exceptionally many irrelevant entries must still be enumerated for sorting; this tradeoff needs measurement on real removable drives. A cap warning means capacity was reached, not proof that another eligible shortcut existed.

## Reproduction and acceptance

`nested_shortcut_budget_is_global_and_independent_of_creation_order` creates actual nested .url files in two equivalent filesystem trees, with forward/reversed file creation order, and calls the exact production visitor with limits of 2 and 4. Before the guard fix, a descendant reaching the limit did not prevent the parent from pushing a later top-level file, so it could return more than the configured count. After the fix, both trees return the same bounded subset in the same order and emit a single capacity warning. These are synthetic filesystem fixtures, not measured human time saved.

The existing `five_thousand_real_shortcuts_remain_discoverable_and_manual_rules_survive_rescan` remains a full SQLite/URL parsing regression for the documented catalog size and manual Always/Never policies. Run all unchanged Windows Server 2022/2025 CI checks, native integration fixtures, source formatting and release guards. Live Windows 11 Explorer, multi-volume directory ordering, UI at 100%/200%, and behavior on extremely slow directories remain unverified.

## Next steps

1. Monitor real-world scan latency only with explicit opt-in or controlled local tests, not telemetry.
2. Assess whether a clear, user-controlled search scope/preview is needed when the global discovery limit is reached; do not silently expand the limit or rearrange user icons.
3. Keep draft organizer PR #98/#101 separate from this catalog-only change.
