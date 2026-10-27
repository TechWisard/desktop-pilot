# Reliable worker health after a backward clock correction — 2026-10-27

## Starting state and evidence

Baseline is released v0.2.17 at `f84dbad91191e644716bca59b97aa3718c8c8f9e`, with passing [main CI](https://github.com/TechWisard/desktop-pilot/actions/runs/40419626937) and verified public release archives. No open PR was present at the start of this investigation. [Issue #15](https://github.com/TechWisard/desktop-pilot/issues/15) recorded a separate outstanding risk. The earlier clock-recovery, user-control, adaptation and idle-heartbeat investigations remain applicable and are not repeated.

**Primary documentation:** [Microsoft — System Time](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) explicitly says Windows time can be adjusted forward or backward and should not be used to measure elapsed durations. [Rust `Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html) is used by the existing worker for its 15-second heartbeat cadence.

**Analogous reports, not direct Desktop Pilot observations:** A [PowerToys/FancyZones user report](https://github.com/microsoft/PowerToys/issues/10376) from March 22, 2021 describes a background process present after sleep while the functionality was unavailable. [PowerToys Command Palette issue #51040](https://github.com/microsoft/PowerToys/issues/51040) describes displayed clock drift over long uptime. These support the need for trustworthy status and clock handling but establish neither this defect's frequency nor its cause on a user's Desktop Pilot installation.

## Source-confirmed defect and real-path reproduction

`ui::health` first checks whether the background process mutex is present, then treats the persisted SQLite heartbeat as stale only if `at.saturating_sub(heartbeat) > 150`. A stored heartbeat **30 days later than the current clock** makes that condition false. If the process remains alive but is no longer making progress, Settings displayed `Automatic` or `Manual` rather than warning. The behavior depends on the actual health function, not on user-visible formatting.

A failure-first branch commit [65518b9](https://github.com/TechWisard/desktop-pilot/commit/65518b9db051ce12cc7b8dcaecc709ae254b70dd) wired two UI regressions and one real SQLite heartbeat-writer regression into the unchanged v0.2.17 code. [Run 40427866774](https://github.com/TechWisard/desktop-pilot/actions/runs/40427866774) on Windows Server 2022/2025 reached the Rust test suite: 208 passed; two deliberately failed specifically because the future timestamp was displayed as `Automatic` or `Manual` instead of `Unresponsive`. The native heartbeat writer regression passed on the baseline. No protective CI checks were disabled. An earlier authoring commit had test-formatting differences; it is not used as behavioral evidence.

## Narrow behavior change

A positive heartbeat is now considered current only when its absolute signed-time distance from the current Windows clock is **at most 150 seconds**. An old or far-future marker is conservatively treated as unverified. This cannot prove that a worker is frozen: a genuine clock correction can cause a brief warning until the next heartbeat. Conversely, simply showing a process mutex as healthy can hide a frozen process. The conservative state avoids falsely claiming successful monitoring.

The existing background writer uses `Instant` and publishes a new actual wall timestamp at its next 15-second due tick, or immediately for a user command; the warning clears automatically on the next Settings refresh. In this boundary case, the Home page explains that the last timestamp is ahead of Windows time and a new signal is required. No new service, timer, polling interval, setting, schema, permission, dependency or telemetry is introduced.

## Scenarios and measurable acceptance

| Production path | Old behavior | Corrected expected behavior |
| --- | --- | --- |
| Alive mutex + saved heartbeat 30 days in future + automatic mode | Automatic (false healthy) | Unresponsive until new heartbeat |
| Same, manual mode | Manual (false healthy) | Unresponsive until new heartbeat |
| Worker absent, even with future checkpoint | Offline | Offline |
| Alive but no heartbeat yet | Starting | Starting |
| Heartbeat drift from −150 to +150 seconds | Existing mode | Existing mode |
| Drift −151 or +151 seconds | Old: −151 warning, +151 healthy | Both unresponsive |
| Fresh persisted heartbeat after monotonic worker tick | Healthy | Healthy again, no restart |
| Tracking paused / learning unfinished / explicit status warning | Existing state once fresh | Unchanged |
| User-managed shortcuts, settings, history and Undo | Preserved | Preserved |

The tests use the real `ui::health` function and `Store`/SQLite; the heartbeat test invokes the exact production `publish_heartbeat_if_due` writer with synthetic monotonic ticks. No Windows clock was changed on the runner. This is a correctness and recovery fixture, **not** observed savings of user time, battery, SSD writes or an end-to-end physical Windows 11 session.

## Verification and continuation

Run the unchanged `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, native Windows integration checks and release safeguards for both Windows runners. Final main CI, tag, release assets and unauthenticated downloads must be checked independently. The test branch deliberately records failure before correction; do not treat it as a finished release.

Remaining higher-impact studies: long project/habit shifts and false recommendations, actual time to first useful placement, external/cloud profile-drive disappearance, prolonged blocked network files, Windows 11 UI at 200% scaling and measured human task time. Existing user preferences and desktop files are not modified by this UI-only health decision.
