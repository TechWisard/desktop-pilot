# Avoid redundant worker state and status writes (2026-10-30)

## Evidence and user problem

Inspected `main` `f73ae1e6f79eb299b3c95dbca0394511cacbe817` (0.2.68). The background monitor checks manually managed shortcuts during its regular maintenance. `reconcile_manual_locked` always entered a SQLite write transaction and wrote the complete `state` even when no file, ownership, position, or policy had changed. The monitor likewise wrote an identical `status` string every maintenance cycle. This repeated work could contend for the single SQLite WAL writer and make Settings reload and re-rank its complete catalog needlessly. **These are source-confirmed redundant database writes; no battery-life, UI-latency, or real-user time saving has been measured.**

Primary SQLite source: https://www.sqlite.org/wal.html documents that WAL permits simultaneous readers but only one writer. The existing UI-control research at [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md) references Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704), whose study of static/adaptive/adaptable menus supports keeping deliberate arrangements predictable, not claims about Desktop Pilot runtime performance.

## Reproduction and change

`src/engine/reconcile_noop_tests.rs` runs **real** `Store`/SQLite WAL, production `engine::apply`, `engine::reconcile_manual` and `native::read_link` on an isolated inert `.url` desktop. A SQLite trigger counts actual `state` UPDATE statements after four unchanged reconciliation calls; a second SQLite connection also holds `BEGIN IMMEDIATE` to check the no-op path does not need a writer. It then simulates a user deletion and confirms the production code still releases ownership, persists Never, and does not remove the original. The status test repeats the actual monitor's publication helper under a second writer, then verifies that a true transition is saved once.

The minimal change returns early **only** if `kept == state.owned` and there are no policy changes or manual events. Any moved/renamed/edited/deleted shortcut follows the existing transaction. Monitor status is published only when its text changes. There is no schema migration, changed polling interval, new process, telemetry, source-file mutation, or relaxation of Undo/preflight checks. The normal 15-second heartbeat and all real state transitions remain unchanged.

## Validation boundaries

Run unmodified CI gates on Windows 2022/2025: `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, native fixtures, README and release guards. The tests establish SQLite writes and ownership correctness under synthetic contention, **not** task completion time on a person's Windows 11 computer. Compare the branch's red baseline run (if available) with the corrected CI before merging. Do not issue a product release before those checks succeed. Physical Windows 11 / 200% DPI, long network outages, multi-thousand catalog startup and in-the-wild task duration remain in issue #22.
