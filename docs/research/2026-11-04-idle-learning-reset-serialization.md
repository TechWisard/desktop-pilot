# Serialize idle learning completion with user resets — 2026-11-04

## User scenario and evidence

A user completes seven learning sessions and, before Desktop Pilot's next idle review finishes, deliberately resets learning or increases the required session count. The older reviewer may already have read seven sessions and decided to mark learning complete. **The user's later choice must win**: the reviewer cannot resurrect pre-reset activity or override the increased requirement.

**Source-confirmed defect:** `automatic_review_with_boundaries` read `Progress` and `Config` separately, called `finish_if_ready` in memory, and saved the old `Progress` with `Store::set` without a writer transaction or a final recheck. Settings uses the same SQLite profile from another connection. WAL permits simultaneous readers and a writer, but the read-before-write sequence did not serialize against an explicit reset.

Primary references: [SQLite WAL concurrency](https://www.sqlite.org/wal.html) and [Gajos et al., CHI 2008, predictability and accuracy in adaptive interfaces](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). The latter motivates preserving predictable user control; it **does not** measure this product, prove the frequency of the race, or establish saved time. Continue prior research rather than repeating it: [adaptive shortcut behavior](2026-10-26-adaptive-shortcuts.md), [idle review write avoidance](2026-10-28-idle-review-write-avoidance.md), [atomic operating-mode choices](2026-10-30-operating-mode-conflicts.md).

## Deterministic failure-first reproduction

The tests in `src/engine/idle_review_tests.rs` exercise the **real** `automatic_review` function, saved `Store` SQLite profile, and an independently opened Settings connection. A test-only callback is invoked after the review's optimistic read and before the completion write; it is not present in release builds.

1. Seven sessions are saved; Settings resets the entire learning record before the review saves completion. Old behavior incorrectly restored seven sessions and a nonzero completion time. Required result: zero sessions, zero completion, exactly one `learning` update (the explicit reset).
2. Seven sessions are saved; Settings raises the required sessions from seven to eight at the same boundary. Old behavior still completed learning and reopened automatic eligibility under the earlier goal. Required result: seven saved sessions, unchanged incomplete state, zero `learning` updates.

[Failure-first Windows CI run 41720488239](https://github.com/TechWisard/desktop-pilot/actions/runs/41720488239) reached the real `cargo test --locked` stage. Both Windows Server 2022 and 2025 **failed exactly the two new behavioral assertions**. Windows 2022 recorded 406 passing / 2 failing tests. Windows 2025 recorded 405 passing / 3 failing tests; its additional, independent failure was `store::tests::concurrent_first_open_preserves_a_complete_profile` with SQLITE_BUSY. Do not misrepresent that unrelated intermittent failure as this regression or conceal it.

## Correction and safeguards

Keep the cheap read-only path for already completed or not-yet-qualified learning. Only when the saved snapshot appears ready for a completion transition, take the SQLite IMMEDIATE writer via `Store::transaction`, **reread the current Config and Progress under the same writer**, re-evaluate `finish_if_ready`, and persist only if `completed_at` actually changes. If another connection reset learning or changed its threshold, no stale completion is written. A later reset also wins because it runs after the writer commits.

Retain the prior no-write review tests, the completed-learning foreground sample preflight, read-only idle reviews, pause behavior, display layout protections, and manual selection/Undo. No new persistent state, migrations, network service, hidden collection, or paid dependencies.

## Verification and limitations

Run the unchanged Windows Server 2022 and 2025 `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integration and publication checks before merging. Actual Windows 11 UI behavior, clock corrections under physical use, and observed task-time reductions remain unmeasured. This is a deterministic production-code race regression, not a user productivity study. The unrelated draft organizer PRs #98 and #101 remain separate.
