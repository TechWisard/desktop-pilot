# Revoking input-hook permission across a stale monitor snapshot (2026-10-29)

## Starting evidence and user impact

Baseline: publicly released Desktop Pilot 0.2.53 at `ff0f101412d6e1ac0eb860ccc3e9dd8b74fffdb5`, successful main Windows CI 40877669358, no open PR at inspection. This continues the separate lifecycle investigation in `docs/research/2026-10-29-manual-only-input-hooks.md` and issue #56; do not repeat the prior broad adaptive-UI study.

Microsoft's primary [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc) documentation says low-level Windows hooks receive callbacks on a message-loop thread. It does not quantify Desktop Pilot's CPU cost, nor does it establish an actual privacy incident. SQLite's [isolation documentation](https://sqlite.org/isolation.html) describes the atomic visibility of committed changes between connections.

## Source-confirmed boundary (not a reported user incident)

The worker reads `Config` once near the start of its loop. Before v0.2.54, `engine::sync_input_hooks` checked persisted `shutdown`, `background_disabled` and `Config.collect` **only when installing new hooks**. When an already installed hook object existed, the helper trusted the older `collecting` argument and returned true unconditionally. If another Settings window committed Manual/pause or Turn off between the snapshot and the hook sync, the worker could re-enable its evidence flag for that iteration even though current persisted permission was false. The normal next loop would eventually correct it; this is a consent-ordering defect, not proof that a user's input was logged.

## Correction

Read the persisted shutdown, disabled and collection flags before trusting *any* existing or new hook on each monitor poll. On revoked permission, disable in-memory input evidence before dropping/unregistering the existing hook object and return false. Retain the post-installation permission check protecting the slower registration boundary. Also use `tracking_ready` rather than the stale `config.collect` snapshot when deciding whether to sample foreground interaction in the same iteration. The existing single monitor thread, native hook message pump, Settings locks and test-only counters remain unchanged.

## Reproduction and preservation

`src/background_entry_tests.rs::stale_collect_snapshot_cannot_keep_existing_hooks_after_permission_is_revoked` drives the production `engine::sync_input_hooks` against two real `Store` connections and a native-parsed inert `.url` shortcut. One connection reads `collect=true`, while the other commits a pause, explicit disabled flag or shutdown. The test supplies the stale argument and counts owned hook objects installed/dropped through the same production helper. It also verifies untouched original URL bytes, exact manual policy and unchanged usage history. On the old code, a pre-existing hook is erroneously retained on the first pause; after correction it is released without installing another copy. This tests ordering and lifecycle, not actual CPU callbacks, keyboard content, human time savings or physical Windows 11 behavior.

Keep unchanged Windows CI gates: `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integration fixtures, README and release publication guards. The public release pipeline must independently validate its x64 executable, SHA-256 assets and matching commit/tag before publication.

## Remaining acceptance

Issue #56 still needs physical Windows 11 callback/CPU measurements and rapid real keyboard/mouse toggles; issue #22 still needs physical 200% DPI input-to-render, true cold process startup, real stalled network roots and long-horizon habit changes. Avoid claiming an instantaneous OS-wide revocation between independent process instructions; this fix closes the observable stale-snapshot gap at the next hook synchronization boundary.
