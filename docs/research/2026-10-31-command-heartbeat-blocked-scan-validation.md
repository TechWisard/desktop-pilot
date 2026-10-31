# Blocked scan heartbeat verification after a manual command — 2026-10-31

## Starting point and evidence

Baseline: main at `87ac2c95c5c0e9fa7773c5c53a7668e35e464baf` (v0.2.76), with the separate draft URL-shortcut correction at `879f29d3e8d3ef2b877c08b6d186d8c95237775e` (PR #102). The source and persisted-Store fixtures already test that a blocking shortcut enumeration cannot stall manual policy processing, heartbeat updates or safe shutdown.

The strict [PR workflow 41225761394](https://github.com/TechWisard/desktop-pilot/actions/runs/41225761394) passed on Windows Server 2022 and 2025. The simultaneous [push workflow 41225755561](https://github.com/TechWisard/desktop-pilot/actions/runs/41225755561) failed only `slow_catalog_read_cannot_delay_manual_policy_heartbeat_or_shutdown` on 2025, although it passed on 2022. This is an **observed CI discrepancy**, not evidence that users experience an 19-second delay.

External relevance, not evidence of this failure in Desktop Pilot: [Flow Launcher issue #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484) describes temporarily unavailable custom shortcuts after startup; [Flow Launcher issue #4385](https://github.com/Flow-Launcher/Flow.Launcher/issues/4385) describes a user-configured search provider causing a freeze. The [Findlater–McGrenere CHI 2004 study](https://doi.org/10.1145/985692.985704) motivates predictable and user-controlled adaptive interfaces, but does not quantify Desktop Pilot responsiveness.

## Source-confirmed timing mismatch

`engine::monitor` forces `publish_heartbeat_if_due(..., true)` **before processing an explicit policy command**. A forced publication resets its `last_written: Instant`; ordinary idle heartbeats are intentionally limited to once per 15 seconds after that reset.

The original combined fixture allowed up to seven seconds for the command but sampled heartbeat only at 19 seconds *from command submission*, then expected 15 seconds of ordinary heartbeat advancement from the **startup** value. If the command completes at, for example, 6.5 seconds, the next ordinary heartbeat is not yet due at 19 seconds. Even a healthy monitor could fail under load. This is a test timing contradiction, confirmed by source inspection; it is **not** a proven production heartbeat defect.

## Production-path correction and acceptance

The same fixture still blocks the real `refresh_apps_guarded` filesystem path and uses the real monitor thread and on-disk SQLite store. It now captures the persisted heartbeat **after the manual policy command finishes** and polls for an additional ordinary heartbeat at least 15 wall-clock seconds later, within a bounded 30-second post-command observation window, while the scanner remains blocked. The scan gate has a 45-second independent timeout; even a command at the fixture's 7-second deadline leaves the observation inside that hold.

It still checks that the explicit Never command completes within seven seconds, the worker shuts down within five seconds while the scanner is held, no stale scan commits after shutdown, the original bytes are unmodified, and the explicit policy survives. The 30-second bound is test scheduling tolerance, not a product service-level guarantee or a claimed user latency. A monitor that actually stops publishing still fails. A single green rerun is not proof that the flake cannot recur.

## Results and remaining work

- Before: the cited parallel CI run reproducibly exposed one 2025 failure; no physical-Windows-user failure is established.
- Correction commit: `adb78098535418b0239b660877a1f05b501c05ea`.
- Required: evaluate strict Windows 2022/2025 Rust tests, Clippy, formatting, build, native integration and release guards on the new branch head; inspect any mismatched runs rather than suppressing them.
- Unverified: Windows 11 interactive behavior under actual slow network drives/OneDrive, end-user time-to-first-use, and human productivity measurements.
