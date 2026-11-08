# Packaged-app identity and temporarily missing launch aliases — 2026-11-08

## Source observation and independent evidence

A Windows Application User Model ID identifies an application for window/shortcut association, not necessarily the command line or saved launch profile. Microsoft's [System.AppUserModel.ID documentation](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-id) describes this grouping. An independent [PowerToys Workspaces report #35187](https://github.com/microsoft/PowerToys/issues/35187) describes confusing two Chrome profiles with different arguments. This is **not** a reported Desktop Pilot bug or evidence of this specific packaged-app scenario. Existing [adaptive-interface research](2026-10-26-adaptive-shortcuts.md) favors predictable, accurate suggestions over fast but speculative attribution.

In current Desktop Pilot source, `engine::observe_match` already includes temporarily unavailable profiles when deciding if a process path is ambiguous. `engine::observe_app_id` did not: it filtered `!valid` records *before* checking uniqueness. With two saved launch identities that expose the same `shell:AppsFolder\\Example.Package!App` value, if one link went offline, the remaining link silently acquired the other profile's foreground evidence. That usage could contribute to a false shortcut suggestion. This is a source-confirmed attribution inconsistency, not a measured customer incident.

## Native regression and change

`unavailable_competing_packaged_app_alias_cannot_claim_foreground_usage` in `src/engine/profile_attribution_tests.rs` uses real Windows `.lnk` creation/parsing, two inert executable targets with the same recorded packaged-app identity, the production SQLite `Store`, the actual app-ID matcher, usage writer, and preview. It compares a single saved launch path, an unavailable second path, restored availability, and an explicit Never rule. It asserts no ambiguous attribution, no usage row or automatic suggestion, unchanged original shortcut bytes and retained manual policy. The single, genuinely unique valid path still resolves.

The correction includes all matching saved identities when checking uniqueness, then only returns an ID if that identity is currently valid. This mirrors the existing executable-profile safeguard. No schema, data migration, telemetry, policy threshold, interface controls, file movement or launch behavior changed. **Before (logical execution of the original predicate):** the one valid identity is returned after the competing record becomes unavailable. **After (expected from the new predicate):** neither saved identity is attributed in that ambiguous state. CI outcome must be recorded separately; no human time-savings claim is made.

## Acceptance and unfinished work

Require the unchanged Windows 2022/2025 pipeline with locked Rust tests, formatting, strict Clippy, native checks and build on the exact commit. Record the results before merge or release. Further field acceptance: real packaged applications that expose the same foreground ID through multiple profile launch paths, restored network shortcuts, longer project switches, and physical Windows 11 sessions with displays and input. Existing organizer draft acceptance remains independent.
