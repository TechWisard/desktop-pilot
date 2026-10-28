# Avoid redundant learning writes during idle reviews (2026-10-28)

## Scope and evidence

Desktop Pilot 0.2.38 at commit 6de377f76d2d28e2f3e53775351d18ba1d5a209c is the tested baseline. PR #40 and the large-import optimization are already released. This is a different production-path defect: engine::monitor calls automatic_review roughly once per maintenance minute when a scan is not active; the method read the saved learning record, checked whether learning had completed, and unconditionally wrote the record back, even in Manual or Observe mode when nothing changed. It can require SQLite's single WAL writer and create spurious contention.

Primary source: SQLite WAL documentation, https://www.sqlite.org/wal.html, confirms that readers and a writer can coexist but only one writer exists at a time. This establishes a concurrency mechanism, not a measured user delay. Existing HCI context: Findlater and McGrenere, A Comparison of Static, Adaptive, and Adaptable Menus (CHI 2004), DOI https://doi.org/10.1145/985692.985704, favors preserving deliberate user control; it does not measure SQLite behavior in Desktop Pilot. The observed waste is proved by production code and the failure-first test, not by these external sources.

## Failure-first reproduction against the actual app

Use real Store::open, a persistent SQLite WAL database and engine::automatic_review; tests do not substitute a model of the reviewer. The src/engine/idle_review_tests.rs fixtures install a real SQLite trigger auditing UPDATE kv of the learning value and execute repeated reviews with saved incomplete and completed progress, switching between Observe and Manual, and an automatic review that is not due. One scenario holds a second real SQLite BEGIN IMMEDIATE writer and checks the first review needs no writer. Another checks that crossing the learning threshold still persists completion exactly once. No shortcuts are executed or moved.

Baseline failure, before production correction:
- Windows 2022 and 2025 red run https://github.com/TechWisard/desktop-pilot/actions/runs/40727832373 : 271 Rust tests passed; all three newly added tests failed as expected (after a separate format-only correction).
- Repeated unchanged Observe/Manual reviews: **10 unwanted learning UPDATEs** instead of 0.
- Completion threshold plus six idle reviews: **7 UPDATEs** instead of exactly 1.
- Three completed, not-due automatic reviews: **3 UPDATEs** instead of 0.

The earlier format-only red run 40727709843 was not functional evidence; it stopped at cargo fmt --check.

## Correction and preserved behavior

automatic_review now writes saved Progress only when finish_if_ready changes completed_at. That is the only field that finish_if_ready can modify. It does not change selection, priorities, policy, background cadence, read semantics, learning thresholds, historical data, Undo, settings or schema. When the first completion genuinely occurs, it is still saved to SQLite before further review. The completed marker remains stable on following calls and across normal restarts. No new dependencies or telemetry.

## Reproduction commands

- cargo test --locked engine::idle_review_tests
- cargo fmt --check
- cargo test --locked
- cargo clippy --locked --all-targets -- -D warnings
- cargo build --locked
- ./scripts/test-native.ps1
- ./scripts/test-release-workflow.ps1

Production-path regression results are verified in the linked PR and release Actions rather than attributed to the HCI study. The test counters establish real SQLite UPDATE counts on test profiles, not time savings or reduced battery drain on a user's machine.

## Continuation

Keep issue #22 (https://github.com/TechWisard/desktop-pilot/issues/22) open: physical Windows 11 interaction and 200% DPI screenshots, remote-source and profile-disk interruptions, cold-start input-to-first-action, long-term project/habit changes, and review/apply behavior under prolonged source stalls need end-to-end acceptance. In particular, manual Apply still explicitly verifies sources, and an unavailable native source may delay that operation. Measure that workflow with a production-path test before redesigning it. No user files, billing, security settings or repository visibility were changed.
