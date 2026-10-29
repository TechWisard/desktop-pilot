# Settings save independence from Windows startup registration — 2026-10-29

## Starting point

Public main `c9d6f799c7fb4363d83177425f506f84e510b7af`, release 0.2.57, Windows checks 40924892363 successful, no open PR at initial inspection. Existing research on predictable adaptive interfaces and manual-control resilience is retained in `docs/research/`, including input-hook recovery and settings/disable races. No repeat broad search.

Windows documents per-user startup configuration in its Run registry key: https://support.microsoft.com/en-us/windows/experience/startup-boot/configure-startup-applications-in-windows . Its availability and permissions are outside an application's unrelated desktop-capacity settings. Our code already updates that registry via `native::autostart`.

Source inspection showed `Settings::save` calls `setup::save_config` for every edited preference, and `setup::save_config` unconditionally synchronizes Windows startup. Consequently, a registry error can block editing desktop slots even if `Config.autostart` did not change. This is a source-confirmed error-handling coupling, not a claim that Windows normally refuses these writes.

## Failure-first reproduction

`src/ui/save_autostart_isolation_tests.rs` uses the real asynchronous `Settings::save` worker, profile startup mutex, `Store::transaction`, serialized `Config`, native shortcut files and real Apply/Undo. A `#[cfg(test)]` error at `setup::save_config` simulates Windows registration denial without altering the host's actual registry. With autostart unchanged, an ordinary slot update must succeed. An explicit false-to-true autostart change must still report the registration error and keep persisted values intact; after lifting refusal it must save. Check that rule, history, original bytes and Undo survive.

Formatting-only failure on the first test commit (run 40928111231) was not behavioral evidence. The formatted failure-first run https://github.com/TechWisard/desktop-pilot/actions/runs/40928317372 failed the new assertion on both Windows Server 2022 and 2025: 304 other Rust tests passed, one failed. The reported failure was `Changing desktop capacity must not depend on an unchanged Windows startup setting: Simulated Windows startup registration refusal`. This is deterministic rejection at the exact existing production save boundary; it is not a measured real-world registry denial.

## Correction and safety

After the existing three-way settings merge, only call `setup::save_config` when `merged.autostart != latest.autostart`. Otherwise persist the validated merged configuration directly with `store.set` in the existing transaction. Keep the startup mutex, conflict detection and the existing guarded registry rollback for genuine startup changes; do not alter `setup::save_config` for disable/setup operations. Test-only fault injection is absent in release.

## Verification and follow-up

The unchanged mandatory GitHub Actions workflow must pass formatting, full Rust tests, strict Clippy, locked build, 28 native integration fixtures and nine release checks on Windows 2022/2025, followed by independently verified main/release artifacts and tag. No release may be claimed from this document alone.

Physical Windows 11 checks of registry-denial environments, 200% DPI interactions, slow network sources, actual first-user-search latency and long-term changes of habits are not reproduced by deterministic tests and remain tracked in issue #22. No human time savings or power claims.
