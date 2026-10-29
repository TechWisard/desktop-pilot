# Direct CLI placement vs explicit background startup (2026-10-29)

## Basis and scope

This investigation follows the published v0.2.49 startup/disabled Settings command fix (PR #52). That correction synchronized **Settings** manual actions with an explicit Start/Turn off. The independent command-line entry points in `src/main.rs` retained a separate preflight: `--apply-initial`, `--apply` and `--rollback` checked whether a background worker was running, then performed the change without retaining the `startup` mutex. `--init` also checked and then imported separately. This left a short check-to-action interleaving with another Settings window explicitly starting a worker.

This is a source-confirmed ordering hazard, not an incident reported by a Desktop Pilot user. The documented reason to prefer predictable stable control is consistent with previous research on adaptive interfaces, especially Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704). An independent 2026 empirical comparison of adaptive menus with static menus measured costs to memorization and cognitive load under changing layouts (Journal of Systems and Software 231, 112598, https://doi.org/10.1016/j.jss.2025.112598); it does not directly evaluate Desktop Pilot or prove human time savings. See existing `docs/research/2026-10-26-adaptive-shortcuts.md` for previous evidence. Windows synchronization semantics: https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject .

## Production-path regression

`src/cli_startup_race_tests.rs` invokes the exact `direct_cli_change` function used by the CLI. It creates an isolated SQLite profile, a real inert Internet Shortcut, a manual Always rule and a genuinely placed quick copy. The test runs an actual Undo from a separate thread while a **test-only** checkpoint freezes the command between its background-state check and execution. Another thread tries the real per-profile Win32 `startup` mutex—the lock an explicit Settings Start would acquire. The baseline allows that mutex acquisition; a correct guarded path refuses it until Undo has completed.

The test checks restored desktop ownership, unchanged source bytes, disabled status and absence of a new background worker. It never launches an unrelated program and never touches the user's desktop. The checkpoint and synchronization registry are test-only. The fixture reproduces the ordering *possibility*; no actual concurrently starting user process or human workload is claimed.

## Minimal correction

Direct CLI Apply/initial Apply/Undo now acquire `startup` for the entire stopped-worker check and action, with existing `changes` locking inside. Catalog initialization takes `startup` while checking worker state and importing. The default GUI flow, taskbar state, shortcut ownership, automatic ranking, history, Undo semantics, local-only data and release safety gates remain unchanged. No new service, dependency, settings or telemetry.

Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked` and existing native Windows/release guards on the standard public repository GitHub Actions matrix. Preserve the baseline red result separately from formatting-only runs. Recheck PR, main, tag and release checks separately before claiming publication.

## Unverified acceptance

This test does not protect against an independently launched `--background` process that bypasses Settings startup synchronization, or measure real Windows 11 interactivity. Test physical Windows 11 at 200% scaling, network/cloud unavailable roots and actual process-start-to-first-useful-search timing under issue #22. Never claim a green CLI mutex test substitutes for those checks.
