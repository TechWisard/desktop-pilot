# Prevent automatic layout changes while an ordinary key is held — 2026-11-04

## Confirmed behavior and user scenario

The existing `native::quiet_for_changes` combines a 15-second `GetLastInputInfo` check, fullscreen and foreground-window checks, and the high bit of `GetAsyncKeyState`. Its held-key check previously enumerated five mouse buttons and only Ctrl, Shift, Alt and Win. It did **not** check ordinary keys. A key held without fresh repeat messages can therefore outlast the idle threshold, even if the user is still interacting with a canvas or an accessible keyboard workflow.

Primary API references: [GetAsyncKeyState](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate) identifies a currently held key through its high-order bit; [GetLastInputInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getlastinputinfo) only reports last input time. A [Windows 11 user report dated 21 September 2025](https://learn.microsoft.com/en-us/answers/questions/5561956/windows-11-automatically-repositions-icons-after-a) describes unwanted icon repositioning in an unrelated Windows setup. [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) demonstrates why prediction accuracy and layout predictability matter to adaptive interfaces. **Neither external source establishes that Desktop Pilot has caused this event for a real user.**

**Source-confirmed gap:** holding Space, an arrow key, a letter, or a function key was invisible to the previous held-key veto when there were no fresh input messages. **Hypothesis:** holding Space to pan a creative workspace, or holding an arrow key with repeat disabled, could overlap an automatic review and lead to an unwelcome shortcut movement.

## Narrow change and regression

- Scan standard virtual-key values `0x01..=0xFE` using the same `GetAsyncKeyState` high-bit predicate, covering ordinary keyboard keys and the original mouse/modifier set; do not track typed content, key names, or any user data.
- Preserve the 15-second inactivity threshold, foreground recheck, fullscreen veto, pre-commit quiet recheck, Settings permissions and user/manual changes. The broader scan runs only when the idle threshold is met (and again during final permission recheck), not on every foreground sampling tick. No new background loop or telemetry.
- `native::tests::any_held_keyboard_key_or_mouse_button_blocks_idle_desktop_changes` executes the production predicate with injected synthetic key states: Space, arrows, F1, a letter, all existing mouse/modifier keys, all released, and excluded endpoints. The *prior* predicate would fail the ordinary-key assertions.
- Existing `engine::idle_review_tests` use real SQLite, shortcut selection and placement to verify that a denied quiet check does not alter files or review checkpoints. These tests continue to guard the same caller path.

**Before:** an ordinary key alone could not veto a review once Windows last-input time became old. **After (logic):** any currently depressed standard virtual key vetoes. This is a preventive, source-derived correction, not a measured productivity improvement.

## Acceptance and limitations

Run unchanged Windows Server 2022/2025 format, Rust tests, strict Clippy, build, native and publication checks. Confirm no regression to idle-review deferral and manual placement. Headless CI with synthetic key-state responses does **not** reproduce Windows 11 accessibility settings, key-repeat disabled while physically holding Space/arrow, RDP, fullscreen transitions, or DPI scaling. Interactive acceptance remains open. Do not merge the separate unfinished desktop-organization feature as part of this correction.
