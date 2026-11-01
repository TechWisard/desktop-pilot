# Recheck active use before an automatic desktop review — 2026-11-01

## Established evidence and scope

This continues [the existing adaptive-interface review](2026-10-26-adaptive-shortcuts.md) rather than repeating the literature search. Findlater and McGrenere's CHI 2004 menu study (https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) and Gajos et al.'s CHI 2008 predictability study (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) support prioritizing stable, explainable arrangements. Neither study measures time saved by Desktop Pilot.

A February 13, 2026 [Fences discussion](https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop) and a January 1, 2026 [icon-order report](https://forums.stardock.com/540297/icon-order-changes-frequently-fences-6-win11) describe unexpected movement in another product. They are reports from other users and **not** evidence of this application's incidence or cause. Microsoft documents [GetLastInputInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getlastinputinfo), the per-session last-input signal used by the existing Windows quiet detector.

## Source-confirmed issue

The maintenance loop called `native::quiet_for_changes()` and passed the Boolean to `automatic_review`. The review then read learning progress, ranked the saved catalog, waited for the exclusive changes lock, rechecked settings and scheduling, but **did not recheck the live idle/fullscreen state** immediately before filesystem placement. If the user returned to work during that interval, a previously authorized review could still rearrange shortcuts. This is a real control-flow gap, not a measured frequency of incidents.

## Minimal behavioral fix

Keep the original early quiet gate to avoid unnecessary work. On the production path, invoke the same Windows quiet detector once more after the final settings and review-due checks and immediately before `apply_locked`. A renewed activity/fullscreen state cancels that attempt without changing the review checkpoint; a later quiet maintenance pass can try again. No new timer, background service, stored input event, telemetry, GUI control or new permission is introduced. Explicit Apply remains unaffected.

## Reproduction and regression

The Windows regression `queued_review_does_not_move_shortcuts_after_the_user_resumes_work` uses the production automatic-review gate, `Store` with real SQLite, a real `.url` parser, an actual test desktop directory, selector and placement code. It simulates a user becoming active at the existing pre-apply boundary, and asserts no copied icon, no change to original bytes, no new owned shortcut and no advance of the review checkpoint. After the test user becomes idle, the same pending review commits one copy; another immediate review remains a no-op. Time/input transitions are synthetic, file and database operations are real. The former implementation had no second quiet read.

## Acceptance and limitations

Run locked Rust tests, format checks, warnings-denied Clippy, locked build, native Windows filesystem integration and release guards on both standard Windows CI runners; record their actual outcomes separately. Recheck physical Windows 11/Explorer with fullscreen changes, multiple monitors and 100%/200% interface scaling before asserting real-world usability. A final check cannot mathematically eliminate a later mouse interaction during the transaction, but it reduces the avoidable stale-permission window. This is not an end-user productivity measurement.
