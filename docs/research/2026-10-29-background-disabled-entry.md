# Background entry after explicit Turn off (2026-10-29)

## Verified starting point

Main `c6c58daca19fde2bc83772ce37a9a61dd489c926`, release v0.2.50, Windows CI 40847631604 successful and no open PR on inspection. The repository is already public with functioning standard Windows Actions. No separate public stand or altered visibility is needed.

This extends established user-control research, especially `docs/research/2026-10-29-disabled-manual-actions.md`, `docs/research/2026-10-29-queue-disable-race.md` and `docs/research/2026-10-29-cli-startup-serialization.md`. Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) motivate predictable explicit control. This external study did not observe Desktop Pilot's defect. Windows Run startup and mutex behavior are documented by Microsoft: https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys and https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw .

## Source-confirmed risk

The actual Windows Run value invokes `desktop-pilot.exe --background --home ...` (native::autostart). Under 0.2.50 the `--background` handler, after obtaining the background singleton, unconditionally wrote `shutdown=false`, heartbeat zero and process ID. It did not check `background_disabled`. An already-spawned/delayed standalone process could therefore enter an explicitly disabled profile after Turn off had completed. That is a source-level defect hypothesis, not a known user incident.

The obvious simple check would break explicit resume: `setup::start_background_locked` used to clear `background_disabled` only **after** waiting for child readiness. The background process must therefore be admitted atomically and the explicit Start transition must be reordered with failure recovery.

## Reproducible production-path coverage

- `src/background_entry_tests.rs` calls the exact registration function used by `--background` on an isolated real SQLite store with a real parsed `.url` original, Always policy and saved usage; the production disable path establishes `background_disabled=true` and `shutdown=true`. Assert that late entry is refused and the profile, history and bytes remain unchanged.
- Authorized entry still resets its runtime heartbeat and PID. The same explicit-start permission helper is tested with a synthetic failed child and then a successful child admission, covering restoration of the prior off state.
- The existing `--self-test` native integration now launches the **actual release/debug executable** with `--background` after disabling its test profile, checks no worker is running and no shutdown/heartbeat/sentinel change, then exercises the real `setup::start_background` explicit resume.
- The first failure-only CI (40849684269) stopped at formatting, not behavioral proof. A formatted failure-first candidate is retained on `fix/background-entry-disabled-20261029` for independent red evidence. Ordinary Windows CI, native checks, strict Clippy, formatting and release guards remain mandatory.

## Scope and limits

The production entry checks `background_disabled` while holding the named background singleton and an SQLite writer transaction, before changing session fields. Turn off holds the same background singleton until the disabled state commits. Explicit Start, serialized by the existing startup mutex, grants permission before spawning and restores `background_disabled=true` and `shutdown=true` if readiness fails. This avoids a startup-mutex deadlock: the child does not attempt to acquire the mutex held by its waiting parent.

No data migration, new persistent state, telemetry, network access, settings, dependency or altered behavior for unambiguously enabled profiles. These checks establish deterministic correctness on Windows runners, not real-world frequency of this race, human time savings or physical Windows 11/200% DPI behavior. Keep issue #22 open for physical network outages, cold process-to-first-search latency and long-horizon usage.
