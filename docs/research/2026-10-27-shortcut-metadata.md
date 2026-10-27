# Refreshing existing shortcut metadata without surprise placement (2026-10-27)

## Context and external evidence

Baseline is public Desktop Pilot 0.2.18 at `c78b6b8cf55b4501037c96d1acc4f32418d03df4`. The completed previous work already established identity continuity, source-outage grace, Undo and Windows-clock recovery. This investigation does not repeat those studies.

Research on adaptive UI predictability and accuracy is documented in Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces* (CHI 2008): https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml . Microsoft documents why preserving launch arguments and manually configured workspaces matters: https://learn.microsoft.com/en-us/windows/powertoys/workspaces . A Flow Launcher user reported missing launch-entry icons (issue #3732, 2025-06-14): https://github.com/Flow-Launcher/Flow.Launcher/issues/3732 . This is *analogous user evidence*, not a report of Desktop Pilot's bug or its cause.

## Confirmed source behavior and testable hypothesis

`refresh_apps` reads each stored source using the actual Windows shortcut parser. When the target identity has not changed and the source is valid, the current code updates only validity and outage timestamp. It leaves an old `App.icon` and `App.icon_index` in SQLite. Scanning the filesystem later skips the source because the known app is already valid. As a result, a changed source shortcut's icon remains stale in the catalog after ordinary Rescan, even though the original bytes are readable.

Engineering hypothesis: updating *only filesystem-derived, non-identity metadata* (icon location/index, and the file-derived name) on a successful same-identity source read preserves accurate local catalog state without resetting frequency, user rules, history, own copy bytes, positions or Undo. It must not attempt to synchronize the desktop copy, which is protected by transactional ownership and may intentionally remain unchanged.

## Reproduction and acceptance

`src/engine/source_metadata_tests.rs` calls the real `native::read_link`, `Store`, `apply`, `refresh_apps`, `preview` and `rollback`, using isolated actual .url and .lnk files on a Windows runner (not a mock source scanner). In each scenario:

1. Import a valid source with a custom icon, mark it Always, record actual usage, and place a managed quick copy.
2. Edit **only** the original icon reference and index, preserving launch target and arguments.
3. Rescan: the catalog must reflect the new icon path and index. The source/target/arguments/ID, manual rule, seed, note and usage rows must remain unchanged.
4. Placement preview must have zero additions and zero removals, the managed copy's bytes and position must remain identical, and real Undo must still delete only the owned copy.

Before the implementation change, both assertions on icon metadata should fail on the released code. The `.lnk` test must call the Windows COM ShellLink implementation, whereas the `.url` test calls the real INI parser. Results and CI run links will be recorded on the branch before merging. No external UI study or personal productivity gain is claimed.

## Outstanding

Full Windows checks, native integration, release verification and physical Windows 11 Explorer/Settings inspection are separately required. Long-horizon project-switch adaptation remains a higher-level separate task.
