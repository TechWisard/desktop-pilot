# Turn off when Windows startup was already disabled — 2026-10-29

## Starting point and prior evidence

Main/release 0.2.58 at `1becd0d3e60391f87e4fe9043389b73feb713c86`; prior main Actions run 40930307760 successful, no open PR at inspection. Earlier work on Settings autostart isolation, Turn off/start synchronization, input-hook consent, manual operations and Undo is retained. The new scenario is adjacent to the documented 0.2.58 settings regression and does not repeat the broad interface literature search.

Microsoft documents per-user Run registrations separately from ordinary app settings: https://support.microsoft.com/en-us/windows/experience/startup-boot/configure-startup-applications-in-windows . Findlater and McGrenere (CHI 2004), https://doi.org/10.1145/985692.985704, document the value of user control over adaptive behavior. Neither source proves a failure in this program; the production call path below and executable regression establish it.

## Source-confirmed cause

`setup::disable_background` always called `setup::save_config`, even when the persisted `Config.autostart` was already false. `save_config` calls the Windows startup registration API before persisting the configuration; an OS refusal could therefore block a user's explicit Turn off despite there being no enabled startup entry to remove. This was independent from the 0.2.58 Settings-save fix, which addressed unrelated preference edits.

## Failure-first reproduction

New `setup::tests::turn_off_without_autostart_ignores_unavailable_registry` calls the actual `disable_background`, real profile startup lock, SQLite transaction, command queue, source URL file and usage store. The existing test-only registration refusal guard stands in for Windows and is cleared before assertions. It verifies both boundaries: startup already off must allow Turn off, while enabled startup still requires successful removal. On either path the original source bytes, catalog and usage remain unchanged.

The first two candidate runs failed formatting before executing tests and are excluded as behavioral evidence. The formatted baseline commit `11aa4efb2483e27aefa5aac64cadc1f83ddf513d`, Windows Actions [40951312081](https://github.com/TechWisard/desktop-pilot/actions/runs/40951312081), reproduced the specific failure on both Windows 2022 and 2025: 305 Rust tests passed, one failed per runner. The assertion reported: `Turn off must not need Windows startup access when startup was already off: Simulated Windows startup registration refusal`. This is a deterministic failure at the actual production boundary, not proof of real registry denial.

## Narrow correction and invariants

Inside the same serialized SQLite transition, validate the disabled configuration and call `save_config` only when the previously saved autostart was enabled; otherwise persist the disabled config directly in SQLite without touching Windows. The persistent `background_disabled` guard still prevents delayed background-entry activation if an obsolete external startup entry exists. Keep the startup mutex, command cancellation, shutdown flag, existing rollback on real startup-removal failures, and all publication checks unchanged.

Do not infer measured user time savings, battery benefits or actual registry-access frequency from this fixture. GitHub-hosted Windows integration and UI tests do not replace a physical Windows 11 200% DPI acceptance run. Real inaccessible network/profile disks and longer habit changes remain in issue #22.
