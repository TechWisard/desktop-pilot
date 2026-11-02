# Refresh taskbar eligibility after Settings changes during a scan — 2026-11-02

## Evidence and user scenario

Continue the existing research on stable adaptive arrangements in [adaptive shortcuts](2026-10-26-adaptive-shortcuts.md), [scan/source races](2026-10-27-catalog-scan-source-race.md), and [concurrent settings](2026-10-30-operating-mode-conflicts.md), rather than repeating their searches. Findlater and McGrenere's [CHI 2004 menu study](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) and [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivate respecting explicit choices and predictable arrangements. Neither paper tests Desktop Pilot or measures this bug's frequency.

A user may toggle **Skip apps already pinned to the taskbar** while a slow catalog scan is reading another drive. The existing production scan captured the old `Config.respect_taskbar_pins` and computed `pinned` metadata from it, but revalidated only the managed-shortcut ownership before committing. A newer Settings choice could therefore be followed by an outdated catalog/pin-verification snapshot. This is a confirmed read/commit race in source, **not** a measured field incident. Full Apply already makes a new synchronous scan; a persistent UI catalog and recommendations can nevertheless temporarily contradict the user's latest setting.

## Fix

Inside the real `Store::transaction` at scan commit, compare the saved taskbar-filter preference with the one used during enumeration. If it changed, publish **none** of the obsolete scan and return `Ok(false)`, using the existing bounded retry path to read the latest permission and recalculate pin metadata. Preserve the original two-attempt limit and cancellation logic. Unrelated Settings changes do not trigger extra scans. No new service, timer, telemetry, cache, user-facing complexity or permission is introduced.

## Reproducible acceptance

The native regression `changing_taskbar_filter_during_scan_restarts_from_current_settings` runs the production `refresh_apps` code with actual `.url` parsing and SQLite commits, injects the real Settings value change after filesystem discovery and before commit, and verifies that the source is reread rather than publishing an obsolete permission snapshot. It then exercises the actual preview, Apply and Undo paths to confirm the manual Always rule and original shortcut bytes survive. These are synthetic event timings on real program paths, **not** real-world performance figures.

Before this correction, the old commit path did not inspect the latest taskbar preference; the same fixture would finish after a single read. The assertion for a fresh scan distinguishes the old and new behavior. Run unchanged Windows Server 2022/2025 strict formatting, locked Rust tests, Clippy, build, native integration and release guards. Live Windows 11 taskbar pin enumeration, interactive 100%/200% UI, and transient removable-drive acceptance remain separate requirements. Do not treat this change as proof of user time saved.
