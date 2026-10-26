# Preserving usage history after forward system-clock corrections — 2026-10-26

## Starting point

Published baseline: 0.2.13, commit aec6f11bed8c7590b7eaf21bcad05c2647da159b; successful main CI https://github.com/TechWisard/desktop-pilot/actions/runs/40380713415. No open PR at the start. The earlier fixes for backward clock corrections, offline shortcuts, stable layout and source aliases remain unchanged.

Microsoft explicitly states that Windows system time may be adjusted forwards or backwards and should not be used to measure elapsed time: https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time . The standard Rust Instant API is meant for elapsed measurements: https://doc.rust-lang.org/std/time/struct.Instant.html . Research by Todi et al. (CHI 2021) describes the costs of surprising interface adaptation: https://doi.org/10.1145/3411764.3445497 . A first-person Windows desktop layout report in PowerToys issue 45170 (2026-01-29) illustrates why unintended losses of stable state matter: https://github.com/microsoft/PowerToys/issues/45170 . These are primary platform guidance, general interface research and an analogous user report, **not** evidence that Desktop Pilot users experienced this exact clock bug or a measured productivity gain.

## Confirmed source behavior and hypothesis

The production Store.housekeeping originally computed a 90-day cutoff from the current UTC wall clock and immediately deleted matching SQLite days rows and persisted session checkpoints. If the clock suddenly moved forward four months, otherwise recent usage would be irreversibly deleted before the clock was corrected. This is confirmed from source and reproduced on the production SQLite path; a physical Windows clock was not modified. Existing backward-clock handling in the foreground/session writer and automatic review scheduler did not cover this forward-time deletion.

A separate concern is an upgraded v0.2.13 profile: it has no previous housekeeping-clock record. The first cleanup after an apparently long period of inactivity should not immediately destroy historic rows.

## Bounded correction

The Store now persists a small local housekeeping-clock checkpoint in its existing key/value table. A forward gap exceeding seven days triggers a seven-day hold on **time-based** removal of usage rows and session checkpoints. Count-based pruning of operational event/command logs still operates. A clock correction of more than one day backwards cancels an obsolete future deadline. An upgraded profile with no cursor and an unusually distant latest saved usage day receives the same first-pass protection. Normal 90-day deletion resumes once the hold expires. A normal short gap keeps the original retention behavior.

Seven days is a conservative engineering threshold chosen to accommodate corrections and long absence; it is not derived from the user studies. A genuine ten-day absence therefore delays cleanup, but not the ranking's 90-day evidence window. If the system clock stays wrong longer than the hold, the routine eventually cleans up; a first pass with no usage evidence cannot establish clock accuracy. This is a bounded safeguard, not an authoritative time service. No telemetry, new settings, dependencies or schema migration are introduced.

## Failure-first reproduction and metrics

Using the real Store/SQLite, the existing housekeeping function was initially exposed behind a controlled-clock test seam without changing its behavior. On Windows 2022 and 2025, run https://github.com/TechWisard/desktop-pilot/actions/runs/40397755590 passed 186 of 188 Rust tests and failed the two expected new regression assertions: after a 120-day apparent forward jump a 30-day-old activity row and session timestamp had both been deleted. The third new test confirmed that normal two-day expiration continued to work. The earlier formatting-only failure was corrected before this behavior test; it is not counted as a defect reproduction.

The corrected scenarios exercise (1) 120-day leap, repeated scans three days later and expiry after seven stable days; (2) leap, correction back and cleanup of genuinely old data without changing an Always rule, note or configuration; (3) ordinary two-day aging; (4) first scan of an upgraded profile with no retention-clock checkpoint. These are deterministic test outcomes and record counts, not real elapsed user time. The test suite also includes native shortcut, undo, UI scale and user-rule checks from the previous version.

## Verification handoff

Candidate is version 0.2.14. Before merging, verify the final unchanged Windows 2022/2025 CI on the PR, covering cargo fmt --check, cargo test --locked, clippy with warnings denied, cargo build --locked, native integration fixtures and publication guards. Release publication on main additionally builds a release executable, checks native fixtures and verifies tag/asset hashes. Record actual run links in the PR. Do not infer release success from this research file.

Unverified: real Windows 11 system-clock changes, an actual 90+ day inactive machine, subjective user trust and observed user-time savings. Further high-priority scenarios remain cold-start time-to-value, changing projects and habits, long network-drive outages, and physical Explorer/150–200% scale inspection.
