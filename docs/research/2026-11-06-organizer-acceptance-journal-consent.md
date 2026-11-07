# Recovery consent must name the current operation (2026-11-06)

## Source observation and scenario

The organizer's **Accept current locations** checkbox was stored as a standalone boolean in Settings. Its truth value survived as the modal remained open, even if another Settings instance resolved recovery A and a new pending operation B became active. The modal looked up the *new* journal ID for its button, while the checked boolean still reflected consent for A. That could authorize accepting B without inspecting B's files. This is a source-confirmed cross-window state mismatch, **not** an observed Desktop Pilot customer incident.

A distinct journal ID is the scope of the user's explicit confirmation. The UI now binds confirmation to that ID; it clears the confirmation if the active recovery changes, clears, or cannot be read. Closing the dialog and successfully resolving recovery still clear the confirmation. No additional dialog or automatic file mutation was introduced.

## Regression and evidence boundary

`src/ui/organization/tests.rs::recovery_confirmation_cannot_authorize_a_different_pending_journal` drives the **real Settings dialog**, production SQLite `organization.active.v1` status and egui pointer events. It simulates operation A confirmed, replacement with B in the store while the same dialog stays open, then a click on B's disabled Accept button. The expected result is **zero recovery jobs**, active B preserved, and untouched desktop files. A transition from pending B to no recovery must likewise clear consent. Synthetic journal identifiers are used only to test UI gating; the test does not simulate a physical Windows 11 Explorer.

Existing HCI research in [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md) motivates predictable, user-controlled changes; it does **not** prove user outcomes for this fix. The earlier [organizer design](2026-10-31-desktop-organization.md) requires explicit inspection before accepting interrupted state. No new broad HCI search was needed to establish this concrete consent-scope inconsistency.

## Verification and remaining acceptance

Run the existing public Windows Server 2022/2025 CI without suppressing tests or lint: locked Rust tests, format, strict Clippy, build, native integration and publication safeguards. Record the exact commit and result on the draft PR after completion. The change remains **draft only** until physically verifying Windows 11 Explorer / actual crash-restart recovery, 100–200% scaling, multi-monitor and redirected/OneDrive desktops. The regression verifies an in-process simulated interleaving, not real-human savings or crash-proof behavior.
