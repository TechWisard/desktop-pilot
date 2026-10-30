# Final command outcomes across restart and Turn off (2026-10-30)

## Motivation and observed source behavior

Baseline is published Desktop Pilot 0.2.61 at `ffc727fb092e2234a85ad6dd705447e299290459`. Its normal Windows 2022/2025 main release CI [40980174457](https://github.com/TechWisard/desktop-pilot/actions/runs/40980174457) succeeded, and the main repository is already public. No open PRs were present when this task began. Preserve the previous late-manual-rescan work from `2026-10-30-late-manual-rescan.md` and the command-vs-disable serialization from `2026-10-29-queue-disable-race.md`.

`Store::finish` unconditionally updated a command by ID. Meanwhile `Store::recover_commands` and `setup::disable_background` explicitly mark pending/running commands as canceled. A delayed prior worker response could therefore replace a terminal cancellation with `done`, leaving Settings with a misleading success message. Calling finish twice also overwrote a first terminal result and appended a duplicate event. This is a **source-confirmed transition defect** and deterministic interleaving, not evidence of a reported real-world incident.

SQLite's [UPDATE](https://www.sqlite.org/lang_update.html) documents atomic WHERE-conditional row selection; [sqlite3_changes](https://www.sqlite.org/c3ref/changes.html) explains affected-row counts. These are the basis for a conditional state transition inside the existing write transaction. Earlier interface-control research from Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) motivates honoring explicit user decisions; it does not establish a Desktop Pilot time saving or a specific SQLite failure frequency.

## Actual regression and correction

`src/store/command_completion_tests.rs` runs the real SQLite `Store` on isolated profiles, including separate connections. The failure-first cases submit and claim a real command, cancel it using the actual persisted startup recovery, then deliver a stale success and stale error; a second case attempts completion before claim and repeats completion after a genuine first success. Require terminal outcome/message and event count to remain unchanged. The additional Turn off test calls the actual settings disable transaction and verifies its cancellation is not overwritten by a late success. No synthetic ranker, user programs or real desktop shortcuts are involved because the bug is entirely in the command-status transaction.

The fix changes one UPDATE to `WHERE id=? AND status='running'`. Its affected-row count controls whether an audit event is appended. This is a compare-and-set transition under the same existing SQLite writer lock. No schema or dependency change; the command IDs, normal monitor completion, manual UI, file changes and background scheduling remain untouched. The pre-existing disable fixture is corrected to claim its deliberately completed command, matching the production monitor's actual sequence.

## Reproduction and acceptance

The initial test commit failed Rust formatting on both standard runners and is not counted as behavior evidence. After formatting, [baseline CI 41003946435](https://github.com/TechWisard/desktop-pilot/actions/runs/41003946435) executed the unchanged production implementation: **both Windows 2022 and 2025 runners passed 308 existing tests and failed exactly the two new regressions**. The canceled job was incorrectly rewritten as `(done, "A stale scan says the catalog is fresh")`, and an unclaimed pending command was incorrectly rewritten as done. These are observed failure-first Windows test results, not a measured production incident. The final Turn off regression and corrected existing fixture are part of the green candidate.

Required for a candidate: standard Windows 2022/2025 README guard, nine publication guards, `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build and native fixtures. Independently require main/release checks and matched tag, source, executable and hashes before release is claimed. Any failed preliminary run must not be described as a verified release.

## Limits and continued work

The deterministic tests prove the database-state transition at an injected late-completion ordering, not the incidence of that race on users' computers or task-time savings. Physical Windows 11, 200% DPI latency, genuine slow network/cloud sources, idle hook CPU/battery measurements, and long-horizon adaptive usefulness remain open in issues #22 and #56.
