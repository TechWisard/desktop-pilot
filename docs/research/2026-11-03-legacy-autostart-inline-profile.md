# Legacy autostart entries with a named profile — 2026-11-03

## Scenario and evidence

Continue the [named profile launch investigation](2026-11-03-launcher-equals-profile.md) and the existing operating-mode/autostart safety notes, without repeating the general adaptive UI literature search.

**Confirmed source observation:** `cli::parse` now supports both `--home DIRECTORY` and `--home=DIRECTORY`. However, `native::command_belongs_to_home`, used by `autostart` to recognize and safely migrate/delete the **old shared** `DesktopPilot` startup value, still recognized only the two-argument form. For a saved legacy Run entry such as `"C:\Pilot\desktop-pilot.exe" --background --home="C:\Profiles\Project Alpha"`, it incorrectly returned false for Project Alpha. Disabling autostart could leave this same-profile legacy Run entry active, or enabling could leave two startup values. This is a proven mismatch between source paths, not a measured prevalence of legacy configurations.

**Primary Windows contracts:**
- [Microsoft: Run and RunOnce Registry Keys](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys): an `HKCU\...\Run` value launches at each sign-in and multiple Run entries can coexist. This explains why an unrecognized old entry matters.
- [Microsoft: CommandLineToArgvW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-commandlinetoargvw): quoted command arguments are split according to Windows backslash/quote rules. The production matcher already uses this API.

An [October 2023 WindowsHelp discussion](https://www.reddit.com/r/WindowsHelp/comments/179f6mc) reports two unexplained startup entries and a successful identification using Autoruns. This is an independently published user report about Windows startup visibility, **not** evidence that Desktop Pilot caused either entry or that this particular defect occurs frequently.

## Correction and reproducible regression

Use the production `native::command_line_args` (Windows API) and `native::command_belongs_to_home` with synthetic command strings for Project Alpha and Project Beta.

- **Before:** the legacy same-profile `--home=...` entry was not recognized, so `autostart` would not remove it when the profile was disabled/migrated.
- **After:** split and inline forms identify the same explicit profile, including path-with-spaces quoting. A different profile is not claimed.
- Conflicting repeated `--home`, missing/empty `--home`, and an option token in place of the path are treated as ambiguous: never authorize deleting a legacy value on that basis. An explicitly different profile overrides the older executable-directory fallback.
- Existing split-form matching and adjacent-profile legacy matching remain supported.

Regression: `native::tests::legacy_startup_detects_inline_named_profile_without_claiming_other_profiles`. Related existing checks: `startup_arguments_roundtrip_spaces_quotes_and_trailing_backslashes`, CLI inline-home SQLite reopening and multi-profile startup isolation.

The regression exercises the actual Windows command-line parser and the production decision that guards registry deletion. It intentionally does **not** mutate the CI runner's HKCU Run key, so an actual live-registry disable/reboot acceptance still needs an isolated physical Windows 11 test.

## Validation boundaries and follow-up

Run standard Windows Server 2022/2025 formatting, Rust tests, Clippy with warnings denied, build, native integration checks and release guards. Compare the regression against the old function as a source-level counterexample, but do not report synthetic checks as real-user time savings. No changes to the current per-profile autostart key naming, user data, startup preference, permissions, or telemetry are required.

Continue the tracked [saved catalog cold-start investigation](https://github.com/TechWisard/desktop-pilot/issues/22) and keep draft organization PRs #98/#101 separate until interactive acceptance.
