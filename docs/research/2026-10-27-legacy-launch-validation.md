# Validate legacy shortcut targets before dispatch (2026-10-27)

## Starting point and evidence

Baseline: released v0.2.20, commit `dae8ab3315b0a15241290eaf0028292b23057b76`. Source, recent merged PRs, branches and main CI were inspected before work. Previous adaptive-UI research, including Findlater/McGrenere (CHI 2004) and Gajos et al. (CHI 2008), is retained in `docs/research/2026-10-26-adaptive-shortcuts.md` rather than repeated.

Microsoft's `IShellLinkW` specification documents shortcut target and working-directory fields: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ishelllinkw . `ShellExecuteExW` dispatches requests through shell handlers: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw . A Flow Launcher issue from March 26, 2025 concerns an analogous broken-shortcut experience: https://github.com/Flow-Launcher/Flow.Launcher/issues/3386 . It is external user testimony, not a Desktop Pilot bug report or a measured productivity result.

## Confirmed production inconsistency

`native::read_link` already determines `App.valid` from the target path, handler and optional working folder. However, before this change, the legacy `--launch` branch checked only source-file existence and unchanged launch identity, whereas `--validate-launch` additionally checked `actual.valid`. Removing an executable while leaving its .lnk unchanged therefore made the actual launch preflight accept an invalid source and dispatch it to `native::open_file`. The ordinary direct Windows desktop copies are unaffected by this legacy command.

A behavior-preserving extraction of the actual `--launch` preflight into `checked_launch_source` was committed with native regression fixtures. On the unchanged behavior, the missing-target assertion failed on both Windows runners in [run 40453464196](https://github.com/TechWisard/desktop-pilot/actions/runs/40453464196): 226 passed and one failed per runner. This is an intentionally red comparison, not proof of a release. Earlier authoring tests stopped at formatting and are not counted as behavioral evidence.

## Correction and scenario acceptance

Both commands now call the same source verification; a target or working folder whose native validation fails returns a bilingual recovery explanation before any Shell dispatch or launch-only counter write. The original link is never rewritten. A temporary outage does not turn an unchanged link into a different identity, and the next request succeeds once the original target reappears. Existing protection against changing targets and self-launch loops remains.

`src/launch_validity_tests.rs` invokes real COM `.lnk` creation/parsing and the real SQLite Store through the production preflight, not a model of the launch rules. Scenarios cover: existing inert executable, target removal without changing link bytes or ID, rejected launch, unchanged stored history, recovery after restoring the target; separately changing link identity to a different existing target; deletion of the source link; valid HTTPS `.url` acceptance without network access. A separate `i18n` regression checks recovery text in both languages. Neither any third-party program nor a real browser is launched by these fixtures.

Regression counts, tests and CI results are engineering correctness evidence, not measured end-user time savings. Windows 11 physical desktop interaction, power use, long network stalls and population-level efficacy remain untested. Continue with longitudinal project/habit shifts and false-placement/churn metrics, intermittently missing profile drives and physical high-DPI GUI review.
