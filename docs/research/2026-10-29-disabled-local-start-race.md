# Disabled local command versus concurrent Start (2026-10-29)

## Starting point and external evidence

Based on released v0.2.48 at `90b5bcc3c919f28096ed696358937388aff0e2ec` after PR #51, with successful standard main CI and no open pull requests at inspection. The main repository remains public and its own normal Windows CI is available; no separate testing repository or duplicated private binaries are needed.

Earlier user-control research and regressions: `docs/research/2026-10-29-disabled-manual-actions.md`, `docs/research/2026-10-29-queue-disable-race.md`, and `docs/research/2026-10-29-save-disable-serialization.md`. The known design goal, supported by Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004, https://doi.org/10.1145/985692.985704), is predictable control and reversibility. That study does not report a Desktop Pilot defect. Primary Win32 documentation for the synchronization boundary: https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject . SQLite writer isolation: https://www.sqlite.org/isolation.html . Neither source alone proves an observed user slowdown.

## Source-confirmed gap and risk

In v0.2.48 `Settings::queue` called `setup::enqueue_settings_command_if_enabled`, which acquired the per-profile `startup` mutex. If `background_disabled` was true, it returned `None` and released that mutex. Only afterward did the separate Settings job open SQLite, verify the worker was not running, and call `engine::command` directly. Another Settings window could explicitly Start in this gap, making the earlier "local while disabled" decision stale. Existing PR #50 protects enabled command INSERT; PR #51 protects saves; neither spans this disabled direct-operation boundary. This is a verified source-level concurrency opening, not evidence of a user-reported race.

## Production-path fixture and correction

`src/ui/disabled_start_race_tests.rs` uses an isolated real URL shortcut, a stored manual placement, the asynchronous Settings queue, Windows named mutex, SQLite, and the existing `setup::disable_background` and `engine::command`. A test-only checkpoint pauses the actual disabled local command boundary. Another thread tests whether it can acquire `startup` while the local action is unfinished. The old code allows acquisition; corrected code must refuse until the action completes. The test also checks the manual Never rule, unchanged original bytes/history/position, disabled and shutdown flags and absence of a new worker.

Correction: `setup::dispatch_settings_command` retains the existing `startup` mutex across mode selection and execution. Disabled actions initialize COM and invoke the established direct command under that mutex; enabled actions start/verify their worker and insert into SQLite before releasing it. The established `changes` mutex, SQLite transactions and Undo remain in effect. There is no persistent cache, new schema, new dependency, extra worker, logging, or telemetry. The test checkpoint and registry are compiled only with `#[cfg(test)]`.

## Validation and limits

Reproduce on the baseline with the exact test and checkpoint; formatting-only failures do not demonstrate the defect. Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `./scripts/test-native.ps1` and existing publication/README guards in the unchanged Windows Actions matrix. A red/green named-mutex test is a deterministic ordering check, not a measured real-user task-time or battery improvement. Final PR, main and release checks are distinct gates. Recheck tag, commit and asset hashes after publication.

Still open: physical Windows 11 at 200% scaling, actual network/cloud root outages, full cold process-start to first useful rendered search and long-horizon project changes. Do not infer these from this race fixture or repeat already completed research.
