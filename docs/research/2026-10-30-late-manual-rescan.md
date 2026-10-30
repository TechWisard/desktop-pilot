# Manual refresh must reflect a fresh filesystem snapshot (2026-10-30)

## Evidence and purpose

Baseline: Desktop Pilot 0.2.60 at commit a0324ff8887b51afb50c1adcc46c271dd3b38bff. At inspection there were no open pull requests. Its Windows 2022/2025 CI and release were successful; the public main repository already provides all needed standard CI. This work continues the cold-start and refresh acceptance [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22), not another compatibility matrix.

The primary human-interface study by Findlater and McGrenere, CHI 2004 (https://doi.org/10.1145/985692.985704), concerns predictable adaptive interfaces and explicit control. It is motivation rather than evidence of Desktop Pilot task-time savings. The analogous [Flow Launcher issue #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484) relates to another product; its reported delay does not establish a Desktop Pilot defect.

## Confirmed production-path failure

The monitor put new rescan command IDs into the existing in-flight scan request list, even if that scan had already enumerated its source folders. Its completion then falsely acknowledged the later requests without observing newly created shortcuts.

The failure-first test engine::deferred_rescan_tests::rescan_requested_after_enumeration_must_run_again_before_reporting_success calls the actual background monitor, native Windows URL parser, scanner, SQLite command queue and persisted catalog on an isolated temporary desktop. A test-only gate blocks the real scanner after enumeration, before commit. A new inert shortcut is created, two rescan commands are claimed, and the old scan resumes. Assert command results, the catalog entry, original bytes, and zero unintended manual-mode placements. The test stops the real worker and removes its profile.

The first test-only workflow 40959470669 failed formatting and is not behavior evidence. After applying Rustfmt's exact changes, [red Windows CI 40959705839](https://github.com/TechWisard/desktop-pilot/actions/runs/40959705839) reproduced the specific defect on both Windows 2022 and 2025: 307 passed, one new test failed on each runner with missing post-enumeration shortcut after reported success. This is deterministic scheduling evidence, not a real-world frequency or user time measurement.

## Correction

Separate command IDs that initiated their scan from those received while it was already in flight. Finish the former using the original scan's result. Coalesce the latter into a single subsequent real scan, and report their completion or failure only when that scan ends. This also avoids a separate new scan per repeated F5. The COM scanner connection, cancellation, maintenance, manual policy priority, original sources, history, Undo, and quiet work boundaries remain intact.

No persistent cache, new setting, dependency, background service or telemetry.

## Verification and limitations

Require the unchanged Windows 2022/2025 full locked Rust tests, formatting, strict Clippy, build, native integrations, nine publication guards, release checks and optional independent archive verification. Existing blocked-scanner manual command/heartbeat/shutdown regression must remain green. The new fixture verifies exactly this interleaving; it does not simulate a physical disconnected network drive or measure human productivity.

Still open: physical Windows 11 at 100%/200% scale, full cold-start process/input latency for large saved catalogs, genuine network/cloud outages, and long-horizon shifts in work/projects. Continue from issue #22 rather than repeating completed research.
