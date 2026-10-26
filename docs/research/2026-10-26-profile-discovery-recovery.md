# Preserve the selected profile during database outages (2026-10-26)

## Scope and evidence

This investigation continues the existing adaptive-interface and short-visit research rather than changing the recommendation weights again. User control is only meaningful if startup consistently opens the profile that holds the user's rules and history.

- Gajos, Everitt, Tan, Czerwinski and Weld, *Predictability and Accuracy in Adaptive User Interfaces* (CHI 2008 Extended Abstracts), experimentally relate predictability and accuracy to satisfaction with an adaptive interface: https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/
- SQLite documents that `SQLITE_BUSY` can prevent reading because another connection holds a conflicting lock. A failed schema read is not evidence that a database does not exist: https://www.sqlite.org/rescode.html and https://www.sqlite.org/wal.html
- A real Flow Launcher user reported fresh settings following an updater copy failure: https://github.com/Flow-Launcher/Flow.Launcher/issues/4288 (opened 2026-02-25). This is an analogous continuity problem in another tool, not evidence of the same root cause or its frequency in Desktop Pilot.

The engineering hypothesis is that silently selecting a different profile after an access failure can hide existing choices and make an adaptive tool appear to have forgotten the user. The source inspection and runtime tests below confirm the wrong-profile selection in Desktop Pilot. They do not measure human time savings, incidence in the field or usability-study outcomes.

## Confirmed failure and minimum fix

In 0.2.7, `profile::adjacent_profile` used `is_file` and discarded SQLite open/query errors with `.ok()?`. For custom installation directory names, a lock, damaged database or obstructed path therefore looked like no adjacent profile. The actual `profile::select` then returned the default LOCALAPPDATA profile. Explicit `--home` and the legacy `DesktopPilot` directory name were already protected.

The fix distinguishes absence or a readable unrelated schema from an inconclusive read. It keeps the adjacent location on metadata/open/query failures and lets the existing validating `Store::open` report the problem. It does not repair, replace or initialize a damaged nonempty database. A readable unrelated database is still skipped. Explicit `--home` remains authoritative. No schema, settings, ranking, placement, dependency or telemetry changes are needed.

## Reproducible scenarios on production logic

`src/profile/recovery_tests.rs` calls the actual profile selector and SQLite-backed `Store`, not a replacement behavioral model.

1. Save custom settings, Always/Never rules, usage history and a manually positioned shortcut. Close the store, acquire a real SQLite exclusive lock, select the profile, release the lock and reopen the selected store. Assert that all saved values survive and no fallback directory is created. Also check the explicit-profile override while the database is locked.
2. Place a corrupt database beside the executable. Require selection of the same directory, a store-open error, unchanged original bytes and no fallback profile.
3. Put a directory with a sentinel file at the database path. Require the same selection, an error rather than a substitute profile, and preservation of the sentinel.
4. On Windows, deny file sharing on a real initialized database. Release the handle and reopen the selected profile, checking its saved marker and absence of a fallback directory.

Existing regressions additionally cover readable foreign databases, valid custom profiles, the default installation name, explicit path validation, legacy migrations, undo and UI settings conflicts.

## Before/after record

Baseline production code: `cb4ca305b1ea1128e9edc193ece29c20ccd8cea4` (0.2.7).
Test-only commit: `ede56756024d6e7874cf1d6b5c4d79708c4e365e`.
Baseline run: https://github.com/TechWisard/desktop-pilot/actions/runs/40315881776

The Windows Server 2022 job compiled and executed 164 tests: **160 passed, four failed**. All four new failures were the intended wrong-profile assertions: the actual result was `fallback/DesktopPilot` instead of the adjacent custom directory. Formatting and all nine release-publication guards passed. Earlier test-authoring commits failed formatting/import compilation; those are not counted as behavioral evidence.

The corrected implementation must pass the same four tests plus the full existing suite, Clippy, build and native fixtures before merge. Record the completed green run in the validation section rather than assuming success from this document.

Reproduce with Rust 1.93.1 on Windows:

```powershell
cargo test --locked profile::recovery_tests
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
./scripts/test-native.ps1
```

## Remaining work and limits

- Complete and record post-fix CI, then release only from a verified main commit with matching version, tag and assets.
- A completely absent custom directory/database is still handled as absence; durable profile identity after an entire drive disappears needs a separate design and tests. This patch does not claim to solve that case.
- No physical Windows 11 desktop, hardware disconnect/reconnect, manual Explorer session or user timing study was performed. CI native fixtures and automated UI layout tests are not substitutes for those checks.
- Retain the earlier backlog of long sequences across project changes, long absences and changing habits. This focused change preserves the data those recommendations depend on; it does not establish that every ranking policy is optimal.
