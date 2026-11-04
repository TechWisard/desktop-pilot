# Unjournaled shortcut preparation cleanup — 2026-11-04

## User scenario and evidence

**Source-confirmed defect, not a measured user incident.** The previous `engine::apply_locked` created a timestamped `history/<id>` directory and copied removal backups before recording a `prepared` transaction. Any error before that INSERT (a temporarily malformed or locked replacement source, an inaccessible directory, failed staging, or a SQLite refusal) returned early without deleting the unjournaled directory. Existing quick shortcuts were not removed, but repeated failed attempts could leave unknown backup files consuming local storage. A restart could not undo or identify them because they had no transaction row.

The already collected [2026-10-26 recovery study](2026-10-26-alias-recovery.md), [2026-10-29 interruption acceptance](2026-10-29-project-change-undo-acceptance.md) and [2026-11-04 first-review study](2026-11-04-first-review-replacement-cap.md) distinguish recoverable journaled operations from ordinary failed preparation. This is a code-path failure; no evidence is claimed about real-world incidence or saved time.

**Hypothesis:** a removable or cloud-synchronized shortcut source can change between a successful scan and a subsequent file copy. The regression injects precisely this deterministic interleaving using the real engine, rather than a separate ranking simulator. Neither the hypothesis nor the synthetic regression constitutes evidence that the failure is frequent.

## Correction

- Require a newly created history subdirectory (`create_dir`, never silently reuse an existing path).
- Keep a scoped cleanup guard while preparation remains unjournaled. This covers all `?` exits during backup and source staging, including failed journal serialization/insertion.
- Disarm the guard only after SQLite accepts the `prepared` record. All failures thereafter retain the original Undo and crash-recovery behavior and preserve the journal's backups.
- Keep the existing identity-verified temporary desktop cleanup and explicit quietness-deferral cleanup. No additional service, telemetry, persistent setting, user-facing UI control or filesystem-wide deletion.

## Reproducible native regression

`engine::tests::failed_preparation_cleans_unjournaled_backups_without_changing_the_desktop` uses a real `Store`, two Windows `.url` shortcut sources, actual initial Apply, policy change, a queued removal and an explicit new application. It changes the new source after catalog verification but before preparing its copy, so the removal backup is written and subsequent source verification fails.

Required assertions: the old managed copy, original source bytes and filesystem identity remain unchanged; no new transaction row exists; the history directory set is identical to the pre-attempt set; no hidden desktop stage remains. A subsequent retry after restoring the source must succeed, and Undo must still preserve originals.

**Before:** an unjournaled `history/<id>` directory could survive the failed preparation. **After (acceptance target):** zero new unjournaled directories, zero changed desktop copies, the existing committed history untouched, and a successful retry/Undo. These are synthetic correctness checks, not measured user productivity gains.

## Verification and release boundary

Run the standard Windows Server 2022/2025 Rust tests, format, strict Clippy, build, native integration and release guards without relaxing conditions. A passed CI does not replace a physical Windows 11 check, UI 100–200% acceptance, cloud-directory contention, or multi-day file-system interruption trials. The existing unfinished desktop organizer PRs #98 and #101 remain separate. No version should be published until the usual checks complete.
