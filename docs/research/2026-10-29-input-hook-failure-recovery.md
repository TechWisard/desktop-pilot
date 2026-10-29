# Input-hook failure recovery — 2026-10-29

## Evidence and scope

Desktop Pilot 0.2.56 runs `engine::monitor` with optional Windows low-level input hooks. Microsoft documents that `SetWindowsHookExW` can fail and that low-level hooks need a message loop and must return promptly: https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc . The dedicated hook thread and manual-only uninstallation were already implemented in PRs #57 and #58. Do not repeat the previous hook/consent investigation.

Source-confirmed failure path at 0.2.56: `monitor` propagated an error from `Hooks::install` via `sync_input_hooks(...)?`; the background worker then terminated. An OS hook registration failure must not make unrelated manual Always/Never/Undo and tray functionality unavailable. This is a *source-confirmed failure-path defect*, not evidence that real users regularly hit denied hooks.

## Change

Retain one hook installation across normal observation, but when *only the actual hook installer* rejects registration, keep the worker running, discard input evidence, show an actionable warning, and retry after at least 30 seconds. Persistent SQLite validation/read failures remain fatal, rather than silently turning a corrupt profile into an apparently healthy one. Manual mode and Turn off still remove hooks immediately, and permission is rechecked on each poll. Repeated retries do not spam events or take a writer on every two-second worker tick.

## Production-path regressions

`src/engine/hook_failure_tests.rs` injects one deterministic OS-installer failure at the real `InputHookRuntime::poll` boundary used by `monitor`. It verifies bounded retries, status warning/recovery, actual SQLite app/history and desktop Apply/Undo while hook acquisition is unavailable, a subsequent successful installation, manual pause/uninstall, and absence of source mutation or invented usage. A second fixture corrupts the persisted config and proves database failures are still propagated. Existing Windows hook-consent, startup-race, native and UI suites remain required.

These tests do **not** emulate the actual Windows access-control policy or measure physical Windows 11 CPU/power; the OS-error boundary is deterministic and injected. A real denial scenario, 200% DPI visual check, and network/cloud-root stalls remain external acceptance work. No new settings, service, telemetry, or dependency.
