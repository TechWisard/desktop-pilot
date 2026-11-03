# Launching named profiles from Windows shortcuts — 2026-11-03

## Context and evidence

Continue the earlier [profile continuity investigation](2026-10-26-profile-discovery-recovery.md) and the [first-use catalog investigation](../../../../../issues/22), without repeating the general adaptive-interface research. The existing source-level rule in `cli::parse` accepted `--home DIRECTORY` and `--out FILE`, but rejected the equally conventional single-token `--home=DIRECTORY` or `--out=FILE` with an "Unknown argument" error. Windows shortcut target fields and scripted profile launchers commonly pass an option and its value as one argument. The latter use is a scenario hypothesis; no user frequency or human time savings have been measured.

[Predictability and Accuracy in Adaptive User Interfaces (CHI 2008)](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivates continuity of user choices; it is not evidence that this particular spelling is widespread. The technical defect is established from the actual invocation parser, not from a model of human behavior.

## Real-path reproduction and correction

Start an existing isolated `Project Alpha` SQLite profile with a saved settings marker. Invoke the *production* `cli::parse` with `--home=<path with spaces> --inspect`, then the *production* `profile::home` selector and `Store::open` on that path.

- **Before:** parser rejects the equals-form argument. Nothing is launched; a saved profile can appear inaccessible from a launcher configured with that form.
- **After:** both `--home=PATH` and `--home PATH` normalize to exactly the same internal argument sequence. The selected path is canonicalized by the existing code, the original SQLite marker is read unchanged, and no alternative default-profile directory is created.
- `--out=FILE` and `--out FILE` share the same normalization. The established JSON-only rule and explicit `--replace-out` consent remain mandatory.
- Mixed duplicate spellings, blank values, option-like values, unsafe non-JSON outputs and duplicate replacement flags are rejected before accessing any profile.

Regression tests: `cli::tests::inline_options_keep_profile_and_report_paths_without_allowing_duplicates` and `cli::tests::inline_home_reopens_the_same_sqlite_profile_instead_of_a_default_profile`. The latter exercises actual SQLite and path selection. Other CLI, profile continuity, recovery and report-file tests remain in place. No database migration, new setting, auto-launch behavior, telemetry or network access was added.

## Verification and remaining work

Run unchanged Windows 2022/2025 `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, native integration and release guards. Confirm final HEAD and both CI jobs, then merge. This synthetic command-line/profile test does **not** establish real-user convenience, first-screen performance or physical Windows 11 behavior. Continue issue #22's cold-start UI and slow-source measurements; keep organizer PRs #98/#101 separate and in draft.
