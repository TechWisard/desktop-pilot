# Explicit consent before replacing an existing JSON report — 2026-11-02

## Evidence, priorities and limits

This continues the prior [report destination review](2026-11-02-json-report-destination.md) and [script protection](2026-11-02-report-script-protection.md), rather than repeating their source research. The change directly addresses a documented remaining risk from 0.2.85: **an existing `.json` file might be another application's settings, not a Desktop Pilot report**. The prior filename safeguard cannot distinguish them. Source inspection of `report::write` at main `32886734e60b762c9e6a9a354323e07b187ea158` confirms unconditional `MOVEFILE_REPLACE_EXISTING` for accepted destinations. The CLI accepted an existing `settings.json` and could atomically replace it. No field incident count or user-time impact is known.

Predictable control and avoiding destructive surprises are consistent with [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml); the study is not evidence that this particular safeguard saves time. A [February 13–March 3, 2026 Fences user thread](https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop) reports unexpected rearrangements and restoration trouble in another product; this is user-reported context about control, not a claim about Desktop Pilot.

## Narrow design and practical benefit

By default, `--out report.json` **creates a new JSON file only**. If a file already exists, the command refuses to run before opening a profile, issuing a placement command, stopping monitoring or changing shortcut state. The user can choose a new filename or explicitly add `--replace-out` to their existing scripted export. The flag is invalid without `--out` and is normalized independently of argument order. Only documented report-producing commands support it. No new UI state, service, network connection or telemetry is introduced.

The final rename uses Windows `MoveFileExW` **without** `MOVEFILE_REPLACE_EXISTING` by default; a file created after preflight therefore also blocks publication instead of being overwritten. Explicit `--replace-out` retains the original atomic replace/sync behavior. Internal `last-error.txt` remains intentionally separate from CLI reporting and keeps its existing permissions and semantics. Existing SQLite, shortcut, symlink and Windows device-name protections remain in place.

## Reproduction and regression

- `report::tests::cli_report_preserves_unrelated_json_and_prevents_racing_replacements` exercises the real report path and an existing synthetic settings JSON. Before this change, the old report writer accepted/replaced it; with this policy, default preflight and actual publication refuse without altering its bytes. It additionally models creation of a conflicting JSON file between a successful preflight and final Windows rename, a fresh uppercase `.JSON` destination, and an explicitly authorized repeatable export. It asserts no temporary files are left behind.
- `cli::tests::existing_json_overwrite_requires_explicit_opt_in` covers flag ordering, duplicates, invalid combinations, file extensions and the normalized invocation.
- Native `--self-test` invokes the actual built executable in a fresh isolated home: `--inspect --home <absent> --out <existing user-settings.json>` must fail without opening/creating the profile or modifying the user file. A separate `--catalog <empty list> --out <existing file> --replace-out` must succeed and write valid JSON without creating that profile.
- Retain existing tests for CLI `--out`, atomic failures, short-name/SQLite guards, hard-linked logs and Apply/Undo side effects. Run unchanged Windows Server 2022/2025 strict format, Rust tests, warnings-denied Clippy, build, native tests and publication guards.

This is a **reproducible synthetic safety regression**, not an observed frequency of user mistakes or proof of improved task time. A malicious process with permission to modify the output directory can still replace paths concurrently; the design prevents unintentional replacement without authorization and does not claim to defend against all hostile filesystem races.

## Continuing work

The separately drafted organization feature (#98/#101), actual Explorer desktop layout, physical removable/redirected directories, the Windows 11 100–200% interactive GUI and sustained multiday recommendation accuracy remain outside this change. If later evidence favors a more ergonomic confirmation mechanism, keep the default refusal and avoid silent overwrite.