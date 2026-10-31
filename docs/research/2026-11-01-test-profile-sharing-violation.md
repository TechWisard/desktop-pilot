# Transient Windows file locks must not produce false test failures — 2026-11-01

## Verified symptom and scope

A real Windows Server 2025 run of Desktop Pilot reported `ERROR_SHARING_VIOLATION (32)` while removing the temporary SQLite-backed profile in `background_entry_tests::a_permitted_background_entry_can_resume_without_resetting_preferences`: [Actions 41153113395](https://github.com/TechWisard/desktop-pilot/actions/runs/41153113395), tracked in [issue #84](https://github.com/TechWisard/desktop-pilot/issues/84). Other tests, and the same test in another run, succeeded. This proves a CI teardown failure, **not** a Desktop Pilot user-data defect or the identity of the competing file handle.

The product's first useful action and interruption handling must not be judged by an unstable test gate. Prior [adaptive-shortcut research](2026-10-26-adaptive-shortcuts.md) and [cold-start confidence scenarios](2026-10-27-cold-start-confidence.md) still provide the product-behavior baseline; no synthetic fixture here is a measurement of end-user time saved.

## Production test-harness reproduction

The new `background_entry_tests::cleanup_waits_for_a_transient_windows_sharing_violation` creates an isolated temporary profile with an actual file and opens a Windows handle that explicitly denies delete sharing. `remove_dir_all` must first fail with Win32 error 32. The test then starts the *same cleanup helper used by existing background admission, Turn off, and input-hook regression tests*. Cleanup must not report success while the handle remains open. After closing it, cleanup must complete and leave no directory.

Existing profile tests now use `cleanup_test_home`: retry only Win32 sharing violation 32, for at most three seconds at 50-ms intervals. Other errors (permission denial, missing directory from a different test, inaccessible filesystem, etc.) still fail immediately. A permanently held lock still fails when the deadline expires; the test does not suppress or rewrite error states. The helper does not change production background permissions, activity collection, SQLite transactions or profile files.

## Gates, limitations and follow-up

Run ordinary Windows 2022 and 2025 Rust tests, native integration, strict Clippy, formatting, release guards, and locked build. Inspect CI logs to confirm the injected file lock and release path completed. Repeat failure-injection only if current CI demonstrates a problem, not because of the historical single failure. This check establishes deterministic *test fixture* behavior on a synthetic handle, not the cause of the original race with another Windows process.

If CI passes, the test-only change can merge without issuing a fictitious application release. Keep [issue #84](https://github.com/TechWisard/desktop-pilot/issues/84) open until ordinary background runs remain stable and actual handle diagnostics distinguish an external indexer from an application handle leak. Priority product acceptance still includes Windows 11 live UI, delayed disconnected sources, project-switch stability, and actual first useful recommendation timings.
