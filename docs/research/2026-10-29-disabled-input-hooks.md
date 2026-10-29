# Avoiding input hooks after explicit Turn off (2026-10-29)

## Starting point and independent evidence

Verified released baseline: Desktop Pilot v0.2.51, main `8565943894ea3366afae955b91128848735d25d9`, standard Windows Actions 40851217063 successful and no open PR at inspection. The repository is public and already has a working release pipeline; no separate test repository or changes to its visibility are needed.

Continue the existing user-control research and reproductions in `docs/research/2026-10-29-background-disabled-entry.md` and `docs/research/2026-10-29-queue-disable-race.md`. Findlater and McGrenere, CHI 2004, https://doi.org/10.1145/985692.985704, motivate respecting deliberate user control, but their study did not test this program. Microsoft's [low-level keyboard](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc) and [mouse](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc) documentation says Windows invokes the respective installed callbacks for input events and notes their latency sensitivity. This is external API behavior, not a measured Desktop Pilot slowdown.

## Source-confirmed defect

The v0.2.51 `--background` path acquired the background singleton and called `activity::Hooks::install()` **before** `register_background_session()` checked the persisted `background_disabled` flag. A delayed Windows Run invocation arriving after a user selected Turn off would briefly register system-wide low-level keyboard and mouse hooks or fail trying to install them, despite being denied a background session afterward. The prior session-denial fix protected saved configuration and usage data; it did not avoid attempting input hooks. No claim is made that disabled input was stored or transmitted: recording stays disabled until the worker enables it, and the rejected worker never starts.

## Production-path correction and failure-first tests

The same `--background` entry now uses `install_input_hooks_if_enabled`: read the explicit disabled flag before attempting to initialize input hooks. Return success without installation when disabled. Continue to call the **existing transactional** `register_background_session` after hook initialization. The second check remains necessary if Turn off overlaps the installation interval, while explicit Start has already granted permission before spawning the child. No new mutex, service, persistent state or data collection is introduced.

`src/background_entry_tests.rs` adds two regression fixtures using a real isolated SQLite `Store` and the exact production admission helpers:

1. After the actual `setup::disable_background` on a profile with a real parsed .url original, saved Always rule and usage history, an injected hook initializer counts attempted calls. It must not be invoked, and the disabled flag, shutdown, heartbeat, input filter, history, catalog and original bytes must not change.
2. An enabled entry is allowed to attempt initialization. The fixture then flips `background_disabled` while that initializer is running; the final existing registration must still refuse admission and must not reopen the background session.

Failure-first evidence: commit `926f18140fae2a9c39ea16b89c6c849c4c9eea01`, [Windows Actions 40853987111](https://github.com/TechWisard/desktop-pilot/actions/runs/40853987111), passed formatting on both Windows Server 2022 and 2025; 295 Rust tests passed and **one targeted regression failed** on each runner specifically because the disabled path still attempted hook installation. The prior formatting-only failure (40853839224) is not behavioral evidence. The new control-flow guard corrects this same failure without disabling any existing checks.

## Reproduction, limits and continuation

Run the unchanged `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `scripts/test-native.ps1` and publication guards on both Windows runners, followed by the usual release-native and package checks. Confirm the merged main commit, v0.2.52 tag and independent public download hashes before describing the release as fully verified.

The injected initializer is used only in a Rust regression fixture, not in the production runtime. Windows native integration still exercises the actual disabled `--background` entry and explicit resume. Tests do not measure physical Windows 11 latency, transient hooks during an unfinished concurrent Turn off, or human task-time savings. Keep [issue 22](https://github.com/TechWisard/desktop-pilot/issues/22) open for true cold-process search timing, real network outages and physical 200% DPI UI tests.
