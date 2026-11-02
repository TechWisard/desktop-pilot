# Returning to work during shortcut preparation — 2026-11-03

## Observation, prior research, and scope

Follow [the prior quiet-boundary regression](2026-10-29-queue-disable-race.md) and the existing idle-review tests rather than repeat the broad study. Gajos et al., [CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), report that predictability and recommendation accuracy both matter to users of adaptive interfaces. A [February–March 2026 Fences user report](https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop) describes icons unexpectedly changing positions after login/display-mode transitions in another product. These are independent external findings, **not** evidence of a Desktop Pilot incident or measured time saved.

**Confirmed production-path gap at source commit `fb2fea4b8e361e4218dc701253af93e032262af2`:** automatic review checks whether the desktop is quiet before starting preparation, but its copy/backup/staging loop can take time on redirected or busy storage. It then writes its transaction journal and publishes shortcuts without confirming that the user is still away. A person returning during that interval can see a late automated layout change despite an earlier idle check.

## Production-path synthetic regression

The test `user_returning_during_copy_preparation_defers_review_without_orphaned_files` runs the unchanged production selector, real `Store`/SQLite, a native-parsed Windows `.url` shortcut, actual backup/staging copies, and an injected quiet-state transition at the third check. The first two checks pass; the third rejects the operation after file preparation but **before** journaling or publishing. Verify that source bytes, user policy, owned state, last-review checkpoint, journal count, desktop directory, and per-operation staging folder are unchanged/empty.

Before the added boundary, only the first two checks existed and preparation proceeded to publish the shortcut. After the change, a declined review removes its own temporary files and remains due. A subsequent quiet pass is verified through the normal apply and rollback path. This is a deterministic correctness scenario; it does not quantify human productivity or actual Windows 11 GUI behavior.

## Minimal correction and boundaries

Change `apply_locked`'s quiet callback to `FnMut`, preserving the existing preflight check and adding a final check immediately after staging but before the transaction journal. On deferral, drop the ownership-checked temporary copy guard and remove only this newly created per-operation directory. Explicit user-triggered Apply still uses an always-true callback. There is no new setting, telemetry, network access, content inspection, or change to manual restrictions.

This check reduces the exposure window but cannot guarantee that a user never returns during a subsequent already-journaled multi-file operation; aborting such a transaction would require the existing durable rollback discipline. Physical Windows 11 exploration, redirected desktop failures, real timing and interactive scaling tests remain open.

## Acceptance

Run the unchanged Windows Server 2022/2025 workflow: `cargo fmt --check`, all locked Rust tests, strict Clippy, locked build, native integration and release safety checks. Do not merge/release before green CI. Keep draft organizer PRs #98 and #101 untouched. No new release is warranted on an unverified branch.
