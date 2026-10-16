# Verification scope

[Русский](verification.ru.md) · [Home](../README.md)

## Automated checks

- Rust formatting and Clippy with warnings treated as errors.
- 31 unit tests covering selection stability, manual precedence, taskbar/context-menu exclusions, independent active days, configuration migration, profile ambiguity, Windows app identity, Steam roots, PIDL bounds, localization, passive-observation thresholds, meaningful sessions, restart continuity and combined day/session conditions, injected-input rejection, foreground-interaction requirements and zero ranking from launch requests alone.
- Geometry matrix: 5 resolutions × 5 grid/scaling configurations × 4 desktop origins = 100 combinations. Occupied and undersized areas are rejected; existing positions are preserved in the model.
- Native integration fixture: shortcut creation and identity, actual Shell launch through an inert fixture, transactional apply/undo, manual edit/rename/delete behavior, interrupted-operation recovery, refusal to overwrite changed files, clean-profile installation and background shutdown, blocked early placement and automatic first placement after the observation threshold.
- Windows CI builds and tests on `windows-2022` and `windows-2025`. CI does not claim interactive Explorer or Windows 10 desktop acceptance; see the actual [run results](https://github.com/TechWisard/desktop-pilot/actions).

- Regression checks cover direct launch after deleting the app catalog entry, byte-identical `.lnk` and `.url` copies, and installed-profile selection across different launch environments.

## Interactive Windows 11 checks

Real desktop apply/undo/reapply, original shortcut byte preservation, existing folder coordinates, settings rendering in both languages, startup registration, background single-instance behavior and source-shortcut identity have been checked. User applications were not launched as part of shortcut validation. The only integration-test launch target is an inert test executable.

Public screenshots use synthetic profiles. Source and release packaging exclude the installed profile, activity history and local backups. Build-machine source paths are remapped in the release executable.

## Not established

Physical acceptance across every display/DPI combination, Windows 10, ARM64, alternative shells, third-party desktop organizers and arbitrary future taskbar formats has not been established. Automated geometry coverage is evidence for the placement calculations, not a promise of universal hardware compatibility. Version 0.1.1 remains a preview.
