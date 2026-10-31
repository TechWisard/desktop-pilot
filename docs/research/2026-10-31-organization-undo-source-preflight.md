# Organization Undo: source disappearance before restore

Date: 2026-10-31. Continuation of draft PR #98; not shipped.

## Reproduced sequence and user impact

After explicit Apply groups two website shortcuts in a category folder, the user may move, rename or replace one of those organized originals before choosing Undo. The earlier Committed-phase preflight checked only collisions at the *original desktop names*. Undo then entered its durable barrier and restored shortcuts in reverse order. If the later shortcut source was missing/replaced, a previous shortcut had already moved, leaving a partially restored desktop and requiring recovery. A moved/replaced category directory caused the same problem. This is a deterministic code-path scenario, **not** measured prevalence or real-user time saved.

The 2008 CHI paper by Gajos et al. links accuracy/predictability to perceived quality in adaptive interfaces (23 participants); it provides design context, not evidence about Desktop Pilot:
https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml

## Correction

On a still-Committed operation, before persisting the Undoing phase or moving any file, validate **every** organized source by persistent Windows file identity and validate each involved category directory by its saved identity. If a source is missing, replaced, or the category directory changed, refuse Undo with an actionable explanation while leaving the journal Committed and the placement writer available. An existing interrupted Prepared/Undoing operation retains its resumable per-file recovery path and handle-based rename checks. Post-preflight TOCTOU checks remain mandatory; same-file edits remain safe.

## Production-path tests

`src/organize/acceptance_tests.rs` exercises the real Windows file handles and SQLite Store, not a standalone simulation:

- A missing Alpha source with Beta still organized: no Beta move, no new pending recovery barrier, no catalog/history changes; restore Alpha to the organized folder and retry Undo.
- A replaced Alpha source: preserve foreign bytes, refuse partial Undo, move the foreign replacement aside and retry with original filesystem identities intact.
- A renamed category folder with a new different folder occupying the original name: reject before any move, retain the new folder's bytes, restore the original folder and complete Undo.

No real user desktop or shortcuts were touched. Strict Windows CI and physical Windows 11/OneDrive/100%-200% interactive GUI acceptance remain separate gates. No new release is warranted before those checks.

## CI regression follow-up

The first full Windows Server 2022/2025 runs on commit `acef00d9` found two formatting differences and one existing regression fixture that assumed every replaced-source Undo must enter the interrupted-recovery state. The new preflight correctly refuses that state transition before moving anything, so the fixture now explicitly verifies Committed-phase retention and retries Undo after returning the identity-matched original. This preserves stronger behavior without weakening acceptance checks. The first run was **red**: 387 passed, 1 failed on each OS (and formatting failed). Validation of this follow-up commit must be evaluated separately.
