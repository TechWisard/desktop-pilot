# One clock instant for preview and outage retention (2026-10-29)

## Starting point and observed behavior

The current `main` was `9122a245a869ceb2041935e1ca05121d1d87b924`, after the project-transition/Undo integration test (PR #46); the last published version was 0.2.43. No open PRs were present when this investigation began. The repository was public and its normal GitHub Actions workflow was working. The optional `TechWisard/finite-checks` repository did not resolve, and was unnecessary.

Desktop Pilot's production `engine::preview_loaded(cfg, apps, days, state, initial, time)` already accepts a snapshot timestamp and passes it into `model::rank`, but `model::select` independently called the wall clock to decide whether an existing, automatically managed shortcut had exceeded the 72-hour unavailable-source grace. These two decisions can therefore disagree at the expiry boundary if a clock correction or a boundary crossing occurs between the calculations. This is a source-confirmed correctness defect, not a measured Windows 11 speed problem. The existing source has prior protections for temporarily unavailable files, manual Always/Never rules, and backwards clock corrections; those protections must stay intact.

Existing primary and product-context sources are recorded in `docs/research/2026-10-26-adaptive-shortcuts.md` and `docs/research/2026-10-28-evidence-based-reasons.md`. The relevant acceptance principle is predictable behavior with explicit control; neither those sources nor external product reports establish a Desktop Pilot time-saving measurement.

## Failure-first reproduction

`src/model/selection_clock_tests.rs` exercises real `Store::open`, SQLite app/state rows, Windows native `.url` decoding and the exact production `preview_loaded` selection. A saved invalid original with a previously managed automatic copy is compared to a valid candidate with verified saved priority. The test computes a preview one second before and one second after the existing 72-hour limit, without advancing the actual machine clock. It also checks manual Always, explicit Never, exact added/removed sets, and unchanged source bytes.

Initial test-only commit `06a2837` failed formatting; that run is **not** counted as behavioral evidence. Formatted baseline commit `816c23d7b086f9e94d6fabf43ae1b38e23f5a589` ran unchanged production selection. Both Windows 2022 and 2025 failed the new assertion: the expired automatic copy remained selected rather than freeing the only slot at the supplied preview time; 284 other Rust tests passed on each runner. [Failure-first Windows CI](https://github.com/TechWisard/desktop-pilot/actions/runs/40795297680).

## Narrow correction

Introduce `select_at` taking an explicit timestamp; the existing `select` entry point delegates to it with the real current time, preserving legacy callers and tests. `preview_loaded` sends **the exact same timestamp** to ranking and selection, so the published preview and the expiry evaluation do not disagree about when they were computed. No extra clock reads, persistent cache, schema change, dependencies, telemetry, file manipulation or new UI controls. Explicit Always and Never rules continue to override automatic retention. Ordinary preview/apply and Undo paths retain the same policy rules.

## Verification and remaining acceptance

Required checks: Rust suite, real Windows native integration fixtures, release guards, README verification, formatting, strict Clippy, locked build and release-package verification on the existing public GitHub Actions workflow. Do not claim the fix from the red build or from a green isolated clock fixture alone; inspect the PR, merge and release checks independently.

The injected future time proves deterministic policy behavior, **not** that Windows actually jumped forwards by that amount or that users saved time. Real Windows 11 cold-start-to-first-interaction, physical 200% DPI input, offline network-profile drives and longer user-habit trials remain open in issue #22. The new fixture is retained for future validation.
