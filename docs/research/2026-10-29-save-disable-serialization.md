# Settings save / Turn off serialization (2026-10-29)

## Starting point and evidence

Based on published Desktop Pilot 0.2.47 at `ebe5b09c00ae0ee72c894f3359756db544b4023c`. PR #50 had already serialized background command enqueue and Turn off; no open PR at the start of this investigation, and the stock GitHub Actions workflow was available on this public repository. `TechWisard/finite-checks` returned 404 and was not needed.

Source-confirmed adjacent issue: the production `Settings::save` in `src/ui.rs` opened a Store, read `background_disabled`, committed merged settings, then called `setup::start_background(home)` only **after** releasing the database transaction and without holding the profile `startup` mutex for the whole sequence. `setup::disable_background` owns that mutex while stopping the worker, cancelling commands and persisting `background_disabled=true`. Therefore an earlier save could compute `restart=true`, another window could complete Turn off, and the earlier save could later reacquire the lock and start tracking again. This is a source-confirmed ordering hazard; it is not evidence of an observed user incident.

The relevant established research on predictable adaptive control is Findlater & McGrenere (CHI 2004), https://doi.org/10.1145/985692.985704. Existing project investigation `docs/research/2026-10-29-queue-disable-race.md` demonstrates the same user expectation for queued commands. Neither implies measured human productivity improvements.

## Production-path failure-first test

`src/ui/save_disable_race_tests.rs` uses the existing isolated profile fixture with actual native URL shortcuts, SQLite, an actual asynchronous Settings save, and the real Windows named startup mutex. A test-only checkpoint after successful save but before optional restart lets the test attempt acquiring the very mutex used by Turn off, from another thread. If available, a competing disable could cross the gap. The test resumes the worker before asserting, then exercises the real `setup::disable_background` and checks the disabled flag, stopped worker, original source bytes, manual ownership and history. This fixture executes actual application operations, not a modelled event scheduler. The checkpoint and its registry compile only under `#[cfg(test)]`.

The first baseline CI `40824293325` stopped at rustfmt and is **not** behavioral evidence. The formatted baseline is commit `ca3f600dd2b13e63384379b5a7b56a2c706f7302`, Actions run `40824405037`; its behavior/result must be read before claiming a failure-first reproduction. The corrected candidate must pass the exact same test and unchanged full checks on both standard Windows runners.

## Correction and limits

The asynchronous Settings save now holds the per-profile `startup` mutex across reading the disabled flag, the SQLite settings transaction, and any conditional background start. It invokes the established locked startup routine instead of reacquiring the mutex after deciding. The existing setup-complete check and warning-on-failed-start behavior are preserved. The order is consistent with Turn off and the previously corrected command enqueue path. No schema, persistent cache, new user configuration, telemetry or new service.

Check unchanged Rust suite, natively executed shortcut/apply/Undo integrations, release guards, README, formatting, strict Clippy, locked build, and release executable consistency before merging/publishing. CI simulation cannot replace a physical Windows 11 200%-DPI test, actual delayed network/cloud drives, long-horizon user adaptation or human time-saved measurements. Preserve the open cold-start issue #22.
