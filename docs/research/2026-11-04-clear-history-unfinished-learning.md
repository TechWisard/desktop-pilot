# Clearing usage statistics must not complete unfinished observation — 2026-11-04

## Practical scenario and source evidence

A user finishes the configured seventh observation session, but the background idle reviewer has not yet committed a permanent learning-complete checkpoint. The user deliberately chooses **Data → Clear usage statistics** before that reviewer runs. The confirmation explicitly promises that unfinished observation resets, while existing rules and shortcuts are preserved. A new period of observation should therefore be required, not silently bypassed.

**Confirmed source defect in 0.2.107:** `engine::command("clear_stats")` called `Progress::finish_if_ready` on the saved pre-clear counts before replacing the record with zero sessions and active days. This wrote a nonzero `completed_at`, permanently unlocking placement using evidence the user explicitly discarded. It is separate from the already corrected concurrent idle-review reset race ([prior research](2026-11-04-idle-learning-reset-serialization.md)).

The human-interface reason to honor the explicit command follows the existing [CHI 2004 comparison of adaptable and adaptive menus](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) and [CHI 2008 predictability/accuracy study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). Neither study measures this product, estimates how commonly this particular sequence occurs or proves time savings. [SQLite transaction documentation](https://www.sqlite.org/lang_transaction.html) describes the atomicity of the real persisted operation.

## Failure-first production-path reproduction

Added to `src/engine/idle_review_tests.rs`, using the real `Store` SQLite database, the real `command(store, "clear_stats")`, the persisted `Progress` and `automatic_review`:

1. Save seven completed sessions but `completed_at = 0`, with a qualifying partial record. Invoke the confirmed clear command. Assert no sessions, no partial seconds, **no** completion checkpoint, `ready == false`, and subsequent idle review does not restore completion.
2. Save a genuinely completed profile with `completed_at > 0`. Clear history, then verify counters are empty, the **original** permanent completion timestamp is preserved, and subsequent idle review remains read-only.

[Failure-first Windows Actions run 41728037194](https://github.com/TechWisard/desktop-pilot/actions/runs/41728037194) ran unchanged formatting and production Rust tests on **Windows Server 2022 and 2025**. On each runner, **409 tests passed and precisely the new pending-clear assertion failed**: expected completion zero, received a nonzero timestamp. The existing completed-profile regression passed. The earlier formatting-only failure was corrected before this behavioral reproduction.

## Minimal correction and related protections

The clear command now copies **only the already persisted `completed_at`** into the reset record, never calls `finish_if_ready` during clearing. Observation completion remains the responsibility of the normal foreground writer or background idle reviewer, which enforce current settings. The existing clear-history transaction still deletes daily scores, clears historical seeds and session checkpoints and resets learning progress atomically.

The existing `src/ui/clear_history_tests.rs` covers more than 1,000 saved shortcuts, explicit Always/Never policies, notes, current placement, original file bytes, existing quick copies and reversible Undo, as well as a forced SQL failure that must roll everything back. Existing idle-review tests ensure a reset occurring *during* completion also wins, and no-op reviews do not take unnecessary writer locks. No telemetry, network dependency, UI control, additional history, or schema change.

## Verification and remaining uncertainty

Re-run the normal Windows 2022/2025 pipeline: `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, README checks, publication guards and native integration scripts. A successful synthetic replay establishes the behavior of the real application logic and SQLite, **not** actual user productivity, interactive Windows 11 behavior, 100–200% DPI usability or a specific reduction in time to the first helpful shortcut. Keep those acceptance tasks open, along with large-catalog and project-change scenarios in issue #22. Keep unfinished organizer PRs #98 and #101 separate.
