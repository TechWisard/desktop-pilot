# Exact launch-profile attribution without guessing (2026-10-28)

## Starting point and evidence

Baseline is released Desktop Pilot 0.2.29, main `70d77f8ffca851454465cc713bff418e1d77792a`. No open PRs were present at inspection. This continues the catalog/reliability investigations in issue #22, while addressing a separate correctness risk in usage evidence.

Windows documents that [GetWindowThreadProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid) returns a window's thread and process IDs, **not** its shortcut launch arguments. The foreground process path is therefore insufficient to identify a browser window launched with `--profile-directory=Work` versus `--profile-directory=Personal`.

Independent [PowerToys Workspaces report #35187](https://github.com/microsoft/PowerToys/issues/35187), opened 2024-10-02, describes multiple Chrome profiles being confused despite different command-line arguments. [Issue #34626](https://github.com/microsoft/PowerToys/issues/34626), opened 2024-09-05, describes the same class of identity problem in Workspaces. These are user reports about another program, not measurements of Desktop Pilot's prevalence or productivity. Earlier primary research on adaptive-interface predictability remains in `2026-10-26-adaptive-shortcuts.md`.

## Production-source defect and before/after

In 0.2.29 `engine::observe_match` first removed invalid catalog records and then accepted an unambiguous matching executable path. For one profile-specific shortcut, that produced a confident profile ID despite having seen only `browser.exe`. The same erroneous attribution occurred if a second profile was temporarily unavailable, and a default plain shortcut could claim usage when a saved parameterized profile was unavailable. Such usage could subsequently affect rankings and automatic placements.

Four new tests in `src/engine/profile_attribution_tests.rs` use actual Windows `.lnk` creation/parsing, the real SQLite Store, production `observe_match` and `record_usage`, ranking/preview and actual apply/Undo for the unaffected plain-executable case. A test-only inert executable is never launched.

Failure-first [run 40613419941](https://github.com/TechWisard/desktop-pilot/actions/runs/40613419941) on Windows Server 2022 and 2025: **250 passed, 3 failed** on the unchanged 0.2.29 matcher plus the new tests; formatting passed. The three failures were unexpected attribution of a single Work profile, a missing competing Personal profile, and a plain shortcut with a temporarily invalid Private profile. The existing ordinary process-to-shortcut scenario passed. The earlier initial test-authoring commit failed formatting and is not counted as behavioral evidence.

## Correction and bounds

The matcher now scans the catalog once, including temporarily invalid records when determining whether a process path has argument-specific variants. If any saved shortcut for that exact target has launch arguments, process-only evidence cannot be credited to a direct executable shortcut. Distinctive verified process roots (for example, a Steam game's own directory) and specific packaged-app IDs use their existing separate matching paths. An ordinary unparameterized, unambiguous executable still collects confirmed usage. No source file, cached history, manual Always/Never rule, placement threshold, UI setting, schema, dependency or telemetry is changed.

This deliberately favors attribution accuracy over guessing. A profile-specific launch may receive less automatic usage evidence where Windows supplies only the executable path; the user can still choose Always manually. The program does not inspect browser content or capture command-line arguments. The correctness tests are synthetic Windows fixtures, not physical Windows 11 human-use or time-savings measurements.

## Reproduction and continuation

On the baseline with only the new test module included: `cargo test --locked engine::profile_attribution_tests` reproduces three failed cases. On the corrected tree run the full unchanged `checks.yml` matrix, including formatting, Rust tests, Clippy with warnings denied, build, 28 native checks, README and nine publication guards. A successful PR is not proof of published tag and assets; verify the main release independently.

Remaining: whole-profile-drive loss, real stalled network folders, physical Windows 11 200% DPI, verified per-profile Windows app IDs when available, and long-term false-placement/churn/adaptation measurements in issue #22. Do not infer measured human productivity from this fixture.
