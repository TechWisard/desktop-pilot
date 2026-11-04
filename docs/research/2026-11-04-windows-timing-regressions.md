# Deterministic scan and replacement-startup regressions — 2026-11-04

## Confirmed CI observation vs. unresolved product question

The initial Windows Server 2025 CI attempt for Desktop Pilot 0.2.108 failed the blocked-catalog retention assertion and a restart-during-shutdown deadline; Windows Server 2022 passed, and the rerun of failed jobs passed. See [run 41747516411](https://github.com/TechWisard/desktop-pilot/actions/runs/41747516411) and [issue #174](https://github.com/TechWisard/desktop-pilot/issues/174). These are confirmed intermittent **test failures**, not established reports of user-visible retention loss, startup deadlock, or measurable impact on users.

### Blocked catalog scan

The test hooks the real monitor's catalog reader before filesystem discovery finishes. The monitor starts that reader before its first housekeeping iteration. Previously, the test checked for already-pruned SQLite rows **immediately after scan entry**. That sampled an ordering which was never guaranteed; the scanner could enter before the actual housekeeping pass. The corrected test polls the real Store SQLite rows and `app_session_times` for up to seven seconds while the scanner **remains blocked**. It still fails if retention cannot happen without releasing the scanner. Subsequent policy, heartbeat, shutdown, stale-scan, and original-file assertions remain unchanged.

### Startup during shutdown

The test owns the real Windows background mutex, requests shutdown, and races a new `start_installed` call. It previously imposed a fixed five-second test deadline even though the program permits up to twelve seconds for an old worker to exit, followed by readiness handling. The test now explicitly confirms that the old worker still owns the mutex before releasing it, waits for a bounded twenty seconds, and verifies that the old shutdown state has been cleared, heartbeat reset, and the attempted replacement launch failed with the expected missing-executable error. A deadlock or a premature success remains a failure.

## Reproducible checks and acceptance

Run `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `scripts/test-native.ps1` and the normal publication guards on Windows Server 2022 and 2025. The altered assertions use production monitor, lock, launch and SQLite code; no model of scheduling replaces them. Repeat the two named tests under load for statistical confidence before concluding that flakiness is eliminated. Passing a single CI run only supports the specific checked build.

**Safety:** No user data, product runtime logic, settings, telemetry, security policy, distribution, or UI is changed. No CI checks are skipped or made optional. Physical Windows 11, high-DPI GUI, and slow real filesystems remain separate acceptance needs. This test correction should not be described as a user-facing new feature or a reason for a new binary release.
