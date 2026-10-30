# Recover abandoned commands without blocking Settings (2026-10-30)

## Evidence and user impact

Baseline: public release Desktop Pilot 0.2.63 at main commit `321267e9ff4b7f01ba233c27eb7add443348b35f`, with a green Windows check. There were no open PRs at inspection. A dedicated test-only branch `fix/stalled-command-recovery-20261030` preserved the failure-first repro. [Windows 2022/2025 run 41028192739](https://github.com/TechWisard/desktop-pilot/actions/runs/41028192739) passed Rust formatting and reached the actual Settings/SQLite scenario: **318 Rust tests passed, one new regression failed on each runner**, on `Settings::busy()`. Earlier run 41027948372 failed formatting only and is not behavioral evidence.

Source-confirmed problem: `Store::recover_commands` marks pending/running commands interrupted when the monitor restarts. If the monitor exits and cannot be restarted, it never reaches that cleanup. `Settings::reload` still counts unfinished SQLite commands and `Settings::busy` disables unrelated settings actions indefinitely, even when `native::is_running("background")` is false. The footer also incorrectly presents an active spinner. This is a reproducible path, not a measured real-world failure rate.

This work builds on the prior user-control research in `docs/research/2026-10-26-adaptive-shortcuts.md` and issues #22 and #56. Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) motivate predictable direct control rather than forcing restart, but do not establish Desktop Pilot task-time savings.

## Correction and actual-path scenarios

An abandoned global command queue no longer freezes unrelated Settings changes when the background process is missing. The Home screen offers an explicit interrupted-command cleanup that neither starts nor installs the monitor. The asynchronous cleanup takes the same startup mutex as launching the worker, verifies that the background mutex is free, then calls existing transactional `Store::recover_commands`. If a monitor appears after the UI snapshot, cleanup refuses to cancel its live commands. The footer reports interruption rather than claiming running work.

Production-path regressions cover (1) a claimed rescan abandoned after worker exit, (2) an actual Settings/SQLite save while the untouched command remains pending, (3) explicit cleanup preserving original shortcut bytes, manual Always rule, historical usage, existing managed quick copy and Undo, and (4) a competing monitor holding the background mutex after the old UI observed it offline. No persistent schema, telemetry, new dependency, Windows startup change, or CI bypass.

## Verification and follow-up

The baseline failure-first [Windows run 41028192739](https://github.com/TechWisard/desktop-pilot/actions/runs/41028192739) is reproducible. The correction must pass fresh locked Rust tests, formatting, strict Clippy, locked build, native integrations and publication guards on Windows 2022/2025; a PR must be rechecked before merge. Only a fully verified deliverable may be released, with main/tag/artifact consistency. Physical Windows 11 at 200% DPI, real network-drive disconnections, long-running project changes and measured task-time savings remain separate acceptance work in [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22).
