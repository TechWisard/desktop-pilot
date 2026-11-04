# Held input must defer unattended desktop changes — 2026-11-04

## Confirmed source behavior and practical risk

The current automatic-review path invokes `native::quiet_for_changes` before a review and again after potentially slow filesystem preparation. The previous guard checked fullscreen windows, 15 seconds since the last input event, and five mouse-button states. It **did not check held keyboard modifiers**. The Windows API distinguishes these concepts: `GetAsyncKeyState` reports whether a key is *currently down* (high-order bit), whereas `GetLastInputInfo` supplies a timestamp for the most recent input. Holding Ctrl, Shift, Alt or Win without generating further messages can therefore look like 15 seconds of inactivity even though the person is mid-interaction.

Primary API reference: [Microsoft, GetAsyncKeyState](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate) and [GetLastInputInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getlastinputinfo). The previous [quiet precommit study](2026-11-03-quiet-staging-boundary.md) established why a later guard is needed. The [CHI 2008 adaptive-interface predictability study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) motivates predictable changes, but it does **not** establish the frequency of modifier holds or a measurable Desktop Pilot benefit. User-reported unwanted movement in [Fences, Nov 6 2024](https://forums.stardock.com/532016/moving-fences-moves-single-desktop-icons-as-well-causing-chaos) is another-product evidence only.

**Hypothesis:** a long modifier hold can overlap the idle review or a slow preparation step. The source path confirms a missing guard, not an observed customer incident.

## Correction and reproducible regression

- Extend the existing Windows idle-preflight/post-preparation guard to include `VK_SHIFT`, `VK_CONTROL`, `VK_MENU`, `VK_LWIN`, and `VK_RWIN`. Continue protecting left/right/middle/X mouse buttons and ignoring the unreliable low-order `GetAsyncKeyState` transition bit.
- Recheck that the foreground window has not changed while the guard measures its geometry, so a newly focused window cannot inherit the old window's nonfullscreen result.
- Keep the existing fifteen-second review safety threshold, scheduling, learning, rankings, Settings and user files unchanged. No new input hooks, personal data collection, telemetry, services or polling loops.
- `native::tests::held_keyboard_modifiers_block_idle_desktop_changes_like_mouse_drags` drives the **same key predicate used by the production guard** with synthetic key states on a Windows runner. Each held modifier and original mouse button must veto the guard; no held key and an unrelated F1 state must not.
- Existing `engine::idle_review_tests` exercise actual SQLite, native shortcut placement and cancellation when the late quiet-check vetoes a queued or prepared review. The new test isolates the previously missing OS-level reason for that veto instead of mocking the entire selection engine.

**Before (source-confirmed):** an extended Ctrl/Shift/Alt/Win hold alone did not veto a layout update if the last input was old enough. **After (acceptance):** each of those holds vetoes, with zero placement during a rejected quiet boundary. Synthetic correctness checks do not prove actual human time savings.

## Verification and remaining work

Run unchanged public Windows Server 2022/2025 checks: `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, native integration and publication guards. A pass validates compiled logic and tests, not physical Windows 11 key-state/focus races, DPI 100–200% UI, sleep/wake or extended human-use trials. Keep organizer draft PRs #98/#101 separate; release only with the normal verified package and assets.
