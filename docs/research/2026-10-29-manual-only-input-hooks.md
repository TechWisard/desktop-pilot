# Input provenance lifecycle in manual-only mode (2026-10-29)

## Context and evidence

Starting release v0.2.52, main `4ddc3db713528ae2329ab6a9d34c0db63dcdd8a0`, CI 40856040785 successful, no open PR. Continue issue #56 and the prior source analysis in `docs/research/2026-10-29-disabled-input-hooks.md`. No repeated broad search was needed.

Primary Win32 references: [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc) and [LowLevelMouseProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc) document that registered low-level hooks receive OS input event callbacks and require a message loop. This establishes callbacks exist, **not** a numerical CPU or energy cost on users' machines. Findlater and McGrenere, CHI 2004 (https://doi.org/10.1145/985692.985704) support preserving explicit user control in adaptive UIs; their study does not cover Windows input hooks.

## Source observation and reproduction

The background entry in 0.2.52 installed both hooks whenever background work was enabled, even with persisted `collect=false` and `automatic=false`. The worker's `activity::set_enabled(false)` prevented usage capture, but it did not unregister the Windows callbacks. In manual-only mode this work has no purpose, and a hook installation error can prevent otherwise useful manual/tray control.

The new failure-first test `enabled_manual_only_entry_must_not_request_global_input_hooks` exercises the actual entry guard with real SQLite Settings state and a counted input initializer. Before the correction the initializer is called once while collection is disabled. The intended outcome is zero attempts.

## Production change

The already existing monitor, which reads saved config every two seconds, now owns an optional `activity::Hooks` object. On an observed change to collecting mode it installs hooks only if not already present; on pause it disables provenance first and drops/unregisters them. On a concurrent Turn off or mode change during installation, persisted state is rechecked before activation and the new hook is dropped. Tray pause takes effect immediately; tray resume waits for hook registration. The background singleton, transactional disabled admission, startup and settings locks remain intact. No separate polling thread, new per-event checks, schema, dependency, telemetry, or configuration needed.

The same `engine::sync_input_hooks` function used by production monitor is tested against actual Store config for a Manual/Manual/Observe/Observe/Manual/Observe/Manual sequence plus a Turn off during installation. The simulated hook factory counts installations and destruction; it is not evidence of actual CPU cost or actual Windows callback volume. Existing native integration also tests the background process and manual commands.

## Acceptance / limitations / continuation

Run `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, README and release guards and `scripts/test-native.ps1` on both unchanged Windows runners. Only merge when green, then validate the actual release EXE and tag/source checks. Verify no new usage samples, no lost user choice, no unexpected positions, and Undo via existing suites.

Still missing: physical Windows 11 200% DPI and real keyboard/mouse event or battery profiling; long-horizon adaptation and offline network roots in issue #22. Synthetic install/drop counts are not user time saved. Keep issue #56 open until a real Windows lifecycle test and CPU measurement confirm whether more changes are justified.
