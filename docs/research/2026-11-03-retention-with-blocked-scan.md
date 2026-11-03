# Retention must not wait for a successful catalog scan — 2026-11-03

## Practical sequence

A person keeps Desktop Pilot collecting usage while a removable or network shortcut source becomes temporarily unresponsive. The dedicated discovery worker correctly allows manual rules, foreground observation and shutdown to continue, but the scanner may stay blocked or return an error for an extended period. Daily usage rows and session timestamps must still be bounded. A scanner failure must not become an implicit decision to keep old personal activity records indefinitely.

This follows the existing [adaptive shortcut research](2026-10-26-adaptive-shortcuts.md) and [blocked scanner validation](2026-10-31-command-heartbeat-blocked-scan-validation.md). Findlater and McGrenere's original [CHI 2004 comparison of static, adaptive and adaptable menus](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) supports making user control and predictable changes explicit; it does not measure this retention scenario. The disconnected-source scenario is a plausible operational hypothesis, not an observed rate of failure among users.

## Source-confirmed failure

Previously `monitor` invoked `housekeeping_sessions` **only** after a scan worker reported successful completion. The ordinary maintenance loop still ran every minute, but when `scan.is_some()` it bypassed the housekeeping call. A stalled or repeatedly failing scan therefore left the 90-day SQLite `days` retention and `app_session_times` pruning without a successful trigger. This is a control-flow defect visible in the production monitor; no user telemetry was used.

## Correction

The existing `Store::housekeeping` and `housekeeping_sessions` operations now run under a separate six-hour monotonic cadence, independent of the scanner's result. The first maintenance pass also runs retention even with a scan in progress. Success resets that cadence; transient SQLite writer contention leaves it due for a later retry rather than terminating foreground collection. Neither source shortcut enumeration nor desktop placement logic changes. The existing seven-day wall-clock correction hold and safe pruning rules remain authoritative.

## Reproducible regression

`slow_catalog_read_cannot_delay_manual_policy_heartbeat_or_shutdown` uses the real Windows scanner gate, monitor, Store, SQLite tables and session checkpoints. Before starting the monitor it persists one day 110 days old and one recent day, plus an expired app-session timestamp. While the scanner remains blocked on its real filesystem boundary, it verifies the old row and timestamp are pruned, the recent row remains, and manual policy updates and worker shutdown still function.

Comparison on the production logic:
- **Before:** retention invoked only after scan completion; with the gate held, expired evidence remained.
- **After:** retention is checked during monitor maintenance without waiting for that scan; the same SQLite evidence is removed while recent evidence remains.
- **Unchanged:** six-hour cadence for normal background maintenance, collection opt-in, user settings, manual choices, originals, historical layout transaction recovery, local-only storage.

## Verification and limits

Run the unchanged public Windows CI: locked Rust tests, formatting, strict Clippy, Windows build, native tests and release guards. This scenario is a deterministic synthetic blocked-scan integration test using actual application code, not evidence of human time savings. Test interruption and recovery on physical Windows 11 systems with genuinely slow/disconnected paths and competing processes; measure retention database size and responsiveness over multiweek operation. No new binary release is warranted until its build, checksums and distribution files can be verified.
