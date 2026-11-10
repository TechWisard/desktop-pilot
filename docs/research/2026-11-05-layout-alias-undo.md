# Explorer path aliases during desktop-organization Undo — 2026-11-05

## Evidence and practical scenario

On the actual Windows Server desktop fixture, Explorer previously returned a personal-desktop path with an 8.3 shortened parent (for example `RUNNER~1`) while the application's canonical desktop root used the long spelling. This mismatch was recorded in `docs/research/2026-10-31-desktop-organization.md`. The organizer already handles it while reading initial icon positions in `src/organize/plan.rs`.

The Undo position planner in `src/organize/journal.rs` did **not** have equivalent handling: it looked up returned icons only by `norm(path)`. Therefore an unchanged original present in the Explorer view under another spelling could be incorrectly counted as missing; its prior position would not be requested. The native `position_many` additionally matches PIDLs against the **current Explorer path**; passing the canonical spelling alone would fail its lookup.

This is a source-grounded failure path and a plausible desktop regression, not a measured incident affecting a specific person. The prior Windows path alias observation is primary project evidence. Microsoft describes multi-item view positioning here:
https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-selectandpositionitems

The wider reason to preserve stable, predictable locations is supported by Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces*, CHI 2008:
https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/
That experiment does **not** quantify this application's user-time savings.

## Correction

Only when a normalized path lookup fails, match a candidate in the *actual* Explorer layout if:

1. Its leaf filename matches case-insensitively.
2. Its open Windows file identity equals the verified original.
3. Its open **parent directory** identity equals the original desktop directory.
4. Exactly one candidate in the view satisfies all three checks.

Pass the matched view path to the existing `native::position_many`; that call still verifies the resulting position. A same-name public-desktop icon is not a substitute, **even if it is a hardlink with the same file ID**. Ambiguous aliases are skipped rather than arbitrarily moving an icon. Exact normalized paths keep their previous behavior. Journal, filesystem moves, SQLite, rollback and user choices are unchanged.

## Reproducible production-planner regression

`undo_resolves_short_parent_alias_but_never_a_public_desktop_hardlink` exercises the actual `restore_position_plan`, production Windows handle identity checks and temporary real files:

- Public directory with a hardlink of the original but the wrong directory identity: **0 position requests, 1 skip**.
- A dot-segment spelling of the *same* desktop parent that differs textually (simulates a short-name alias without requiring 8.3 generation): **1 position request with Explorer's own path, 0 skips**.
- The same alias icon already in its desired position: **0 needless moves, 0 skips**.
- Two view entries referring to the same identity: **0 arbitrary moves, 1 skip**.

The prior planner regressions for peer swaps, stationary blockers, duplicate destinations and manual returns also remain required. These are synthetic position requests, not evidence of actual Explorer cooperation or seconds saved.

## Acceptance boundary

Keep this on draft organizer PR #98. Run standard Windows Server 2022 and 2025 Rust tests, format, strict Clippy, build, native integration and release guards without bypasses. Real Windows 11 Explorer UI, 100%/200% scaling, OneDrive redirection, multiscreen behavior and interruption/restore still need physical acceptance. Do not release this feature based on the planner simulation alone.
