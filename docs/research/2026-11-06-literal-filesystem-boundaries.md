# Literal Windows paths versus environment-variable templates — 2026-11-06

## Why this matters

Desktop Pilot already distinguished raw copied shortcut paths from expanded environment-variable templates at the import boundary (v0.2.121). Follow-up inspection of the **production filesystem and native paths** found an inconsistent use of the expanding `norm` function after paths were already read from disk or resolved by Windows. A real sibling directory named `%USERNAME%` can coexist with a directory whose name equals the current Windows username. These are different files, but environment expansion previously made several internal comparisons treat them as the same location.

The source-confirmed risks were: shortcut discovery skipping one real directory or file; desktop/staging path boundary checks accepting a file from the sibling; duplicate ownership/recovery comparisons and GUI-position lookups confusing distinct paths; and profile lock/autostart identifiers colliding. A refused boundary check is the safe result. There is no claim that this case has been observed in end-user installations.

## Decision

Only expand environment-variable templates at deliberate input or launch-target boundaries. Use `norm_literal_path` for already-materialized paths: desktop root and journal paths, scanned original shortcut files, managed copies, on-disk path sets, Explorer coordinates and profile mutex/autostart identity. Do not change URI normalization, launch-target expansion, manual policy, ranking, preferences, existing databases, or user files.

This preserves the established priority of predictable controls over surprising automatic movement: Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704), and Gajos et al. (CHI 2008, https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). Neither paper measures this software or the frequency of literal-percent directory names.

## Reproduction and synthetic checks

- Create two actual Windows directories beneath an isolated disposable test profile: one named after `USERNAME`, one literally named `%USERNAME%`; place distinct `.url` files in each. Both must survive the **real** `discover_shortcut_roots` filesystem scanner, regardless of the shared discovery limit. Before this change, the second root was skipped.
- Point `safe_file` and `safe_staging` at the literal-percent sibling while supplying the other as the desktop root. The original bytes must remain untouched and both attempts must fail. Before the change, the path check could accept the sibling.
- Verify production `lock_name` distinguishes both profiles while case-only spelling changes retain one lock identity. A legacy startup command for the literal path must not belong to the other profile.
- Retain regular CLI, import, scan, undo, saved-source, Windows Explorer, and native regression suites.

**Synthetic before/after acceptance:** scanned files from two roots: 1 → 2; foreign-path guard rejection: no → yes; collision of per-profile mutex name: yes → no. These numbers come from the constructed fixture, not user-time savings or field telemetry.

## Boundaries and further work

The on-repository Windows 2022/2025 pipeline is the required verification for code and native integration. This fixture does not demonstrate physical Windows 11 interactive behavior, OneDrive redirected desktops, or interruption after a power failure. The desktop-organizer draft remains separate and requires its existing acceptance gates. Future work should independently test copied literal-percent paths in full Settings search, preserving intentional expansion for entered *templates* without merging actual file identities. No new tracking, networking or billing is introduced.
