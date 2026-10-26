# Verification scope — 0.2.11

[Русский](verification.ru.md) · [Home](../README.md)

## Additional 0.2.11 regressions: backward clock corrections

The actual `record_usage`/SQLite session writer and `automatic_review`/desktop apply logic are tested with persisted checkpoints that are two hours, 14 days, and 21 days ahead of the corrected wall clock. Confirmed new usage resumes without a lost first session; a 30-second adjustment does not inflate sessions; a normal session gap is enforced and a corrected review does not immediately recur. A combined real Store/shortcut-file fixture exercises a user-chosen **Always** copy while automation is disabled, the desktop is busy and it becomes quiet again. The file path, bytes and identity remain unchanged and user opt-out is honored.

The failure-first Windows runs [usage](https://github.com/TechWisard/desktop-pilot/actions/runs/40362314156) and [review scheduling](https://github.com/TechWisard/desktop-pilot/actions/runs/40363233853) show the earlier issue. The first corrected candidate [passed on both Windows runners](https://github.com/TechWisard/desktop-pilot/actions/runs/40363813409) with 177 Rust tests, 28 native checks, nine publication guards, formatting, strict Clippy and debug build. The additional manual-choice regression and versioned release have their own CI gates and must be checked independently. These tests do not change the physical system clock or prove savings in users' work time. [Primary sources, reproduction and open limits](research/2026-10-26-clock-correction-recovery.md).

## Additional 0.2.9 regressions: brief SQLite contention and unexpected worker stop

A second actual SQLite connection holds a write transaction while the production usage writer attempts to record confirmed activity. One fixture releases the lock after the first failed transaction: the bounded retry writes exactly 10 seconds, one session and one committed in-memory checkpoint. Another holds the lock across all three attempts, verifies no partial usage or session state, then releases it and retries successfully. An existing abort-trigger regression now explicitly ensures that a **non-lock** constraint error is attempted only once and rolled back. The Home status regression distinguishes a stopped monitor's real error from stale healthy text, an active process and an intentionally disabled service.

[Windows branch verification](https://github.com/TechWisard/desktop-pilot/actions/runs/40331163215) passed formatting, **170 Rust tests**, Clippy with warnings denied, debug build, 28 native integration checks and release-publication guards on **both Windows 2022 and 2025**. The versioned release build and packaged asset validation run separately. The fixture uses the genuine Store/SQLite transactions, but does not test a physical disk failure, a full process+tray failure-and-restart cycle, new 150–200% error-state screenshots or real-user task times. No telemetry or user-file changes. [Research and follow-ups](research/2026-10-26-worker-lock-recovery.md).

## Additional 0.2.8 regressions: capacity reduction and interrupted usage writes

Real production `model::rank`/`select` and `engine::refresh_apps`/`preview`/`apply` fixtures expose a combined edge case: two already-owned shortcuts, one temporarily missing but historically high-ranked, followed by a deliberate reduction from two places to one. Previously the available shortcut was removed and the offline copy remained. The policy now discards the non-manually-protected unavailable copy first, retains the available file at its existing path, and still honors the explicitly selected **Always** exception. Grace retention without reducing capacity is unchanged.

An isolated SQLite trigger simulates a failed checkpoint write after usage counters were written inside the same transaction. `record_usage` now updates its in-memory last-session cache only after the transaction commits. This preserves a coherent state for a retry without claiming that disk failures can always be recovered automatically; the monitor currently exits on an unhandled write error.

**Before:** [formatted regression commit and Windows run](https://github.com/TechWisard/desktop-pilot/actions/runs/40325563091) — 164 existing tests passed; all three new regressions failed on Windows 2025. The initial test-only commit was reformatted before this run. **After:** [verified fix run](https://github.com/TechWisard/desktop-pilot/actions/runs/40326209390) — both Windows 2022/2025 jobs passed formatting, all 167 Rust tests, Clippy with warnings denied, debug build, native fixtures and release-publication guards. The final release run validates its own packaged executable and assets separately. No physical Windows Explorer trial or user time-saving study is implied. [Research and outstanding scenarios](research/2026-10-26-capacity-and-usage-recovery.md).

## Additional 0.2.7 regressions

Production `UsageAccumulator::sample` now keeps bounded partial confirmed foreground observations across short visits to multiple applications. New tests replay three six-second visits each to two alternately used apps: the initial ambiguous tick of every visit is excluded, but the remaining 12 seconds per app are recorded through the real `record_usage` and SQLite history with only one session. Rapid one-tick switches do not count; inactive gaps beyond 20 minutes and stalled 600-second ticks discard pending partial evidence; 300 distinct identities cannot exceed the 128-app in-memory bound. The earlier uninterrupted-long-use regression is retained. This is a deterministic policy test, not user timing evidence; no physical desktop or opt-in user field study has been performed. See [short foreground visits research](research/2026-10-26-short-foreground-visits.md).

## Additional 0.2.6 regressions

The existing real `engine::refresh_apps`, `model::select`, `engine::preview`, and `engine::apply` paths now retain a previously published copy if its unchanged source or target becomes unavailable within a 72-hour grace interval, without treating the app as eligible for a new copy. Two production-policy tests cover protection of existing positions, refusal of new placement, expiry, `Never` and deliberate slot-capacity reduction. Two in-process engine tests remove and restore a shortcut original or its ordinary local target and assert preserved file identity, byte equality and recovery of valid status; the existing invalidation regression covers a changed URL identity and expiry. All of these are inert Windows fixtures in isolated directories. They do not prove physical drive disconnect handling, real Explorer icon placement or any measured savings in human task time. Research and remaining scenarios: [transient source unavailability](research/2026-10-26-transient-unavailability.md).

## Additional 0.2.5 regressions

The actual model::select function is exercised by two new scenarios: an established manually protected shortcut with three vacant places, and two owned shortcuts with two vacancies and enough challengers to test the replacement cap. The vacant positions can all fill during the same review, while only one existing shortcut may be displaced when the configured limit is one. The existing ranking, manual-protection, file-ownership and rollback tests continue to apply. These in-process fixtures do not measure time saved for real users or prove that a particular physical desktop has space available.

## Additional 0.2.4 regressions

Windows fixtures now exercise the actual catalog, App ID recognition, usage, ranking, preview and copy of a valid `shell:AppsFolder` shortcut; verify unchanged original bytes and a subsequent explicit `Never` rule. Settings search fixtures cover distinct launch profiles, working directories and user notes. They do not establish actual launching of every URI handler or packaged application. The 0.2.3 screenshots remain historical evidence, not a new 0.2.4 screenshot campaign.

## Release gates

The Windows workflow checks formatting, runs the test suite, rejects Clippy warnings, builds the executable and runs `scripts/test-native.ps1` on both `windows-2022` and `windows-2025`. Publishing depends on both jobs succeeding. The optimized release executable runs the native fixture again before packaging. The package script checks its Windows product version against Cargo metadata and produces SHA-256 checksums for both archives. The release links the C runtime statically to avoid a separate Visual C++ Redistributable requirement.

Nine isolated cases exercise the publication script: a new release, draft continuation, an annotated tag, a mismatched tag, an already published release, an incorrect checksum, a missing asset and a main-branch change before or during upload. Publication verifies the commit, file set, sizes and SHA-256 digests through the GitHub API. A mismatch leaves the release unpublished.

[Actual workflow results](https://github.com/TechWisard/desktop-pilot/actions/workflows/checks.yml) are the authoritative record for a given commit. The release archives are built from the published tag's commit.

## Regression coverage

- Shortcut identity, exact `.lnk` / `.url` copies, case-sensitive URL paths and arguments, Windows names and duplicate display names.
- Stable ownership through rename, deletion and manual edits; byte-identical user files cannot be silently adopted or overwritten.
- Prepared and partially completed operations, interrupted undo, staging collisions and recovery after process interruption.
- Atomic configuration and catalog writes, nested transaction rollback, old-profile migration, unrelated database refusal, corrupt-data reporting and command recovery.
- Ranking stability, manual precedence, independent active days, restart continuity, measured elapsed time, sleep gaps and learning thresholds.
- Foreground interaction evidence, stale/injected input rejection and no ranking increase from a launch request alone.
- Profile selection, quoting, installation and replacement, preservation of profile data, and refusal to overwrite an unrelated executable.
- UTF-8/BOM, UTF-16 and INI parsing; taskbar argument matching, Steam path bounds and Windows resource handling.
- Settings merging, conflict handling, saved versus draft state, operating modes, search, interface scale and localization that preserves filenames and paths.
- Consecutive undo of replacements at the same shortcut path, including rejection of a byte-identical user file; atomic new-database initialization and concurrent first opens.
- Cancelling old queued commands on restart, long initial learning with alternating brief and qualified days, and unchanged ranking results after optimizing large-catalog processing.
- Preserving complete disable through settings preparation and updates, restoring background work after a failed executable replacement, refusing installation over a newer version, and retrying an unavailable tray.
- Atomic report writes, retaining an existing report when replacement fails, and protecting the database even through its Windows 8.3 short name.
- Correct undo order after a backward clock correction; retaining a working copy when the original is temporarily unreadable, including a real file-sharing denial.
- No extra sessions after a brief switch following an hour of use or after a restart; keeping shutdown signalled until the process releases its instance.
- Waiting for the previous worker to stop and the new worker to become ready; resolving paths and positions from the same Shell identifier, including a virtual desktop item.
- Nonfatal Start menu shortcut failure, retry and preservation of unrelated files; no diagnostic writes in rejected folders and safe replacement of a hard-linked error log.
- Shared keyboard/selector zoom persisted across reopening; search and pagination with 3,000 apps, rule/ranking refresh, and successful/failed rule confirmation.

The geometry matrix covers **5 resolutions × 5 grid/scaling configurations × 4 desktop origins = 100 combinations**, including occupied and undersized areas. Existing positions are preserved by the placement calculations.

## Native fixture and interface checks

The native fixture uses new isolated `test-*` profiles. It checks 28 end-to-end conditions: shortcut creation and identity, a real Shell launch of an inert test executable, operation apply/undo/recovery, original byte preservation, manual rename/delete/edit handling, refusal to overwrite modified files, clean installation, background startup/shutdown, observation-only behavior and the initial learning threshold. Additional cases cover cancelling stale commands and completing a fresh command, complete disable and explicit resume, preserving disable when preparing Settings, database protection during report export, catalog diagnostics without creating a profile, restoring background work when an open file handle actually prevents executable replacement, replacing an already stopping worker, and three cases preserving files during error reporting. It does not launch user applications or modify the user's desktop/startup entries.

Local checks also run the Windows executable under Wine. The 0.2.3 interface matrix contains **24 scenarios and 123 screenshots**: Russian and English Home, Apps, all five settings pages and History at 1120×820, small 720×540 windows at 150–200% interface scale, and 3840×2160 captures at 125%, 150% and 200%. The three setup steps and long pages are checked from both the top and bottom at 200%. Separate captures cover app details, a pending rule, search and the final page of a 3,000-app catalog, import and long messages. Layout regressions also check all five settings pages at 360 and 480 logical pixels wide. Screenshots use fictional profiles and a supplied desktop diagram. These checks verify the rendered application, not a physical Explorer desktop.

A separate local upgrade fixture starts the published 0.2.2 executable, then opens 0.2.3 with the same isolated profile. It verifies replacement of the running worker, the installed executable hash, retention of settings and usage history, an untouched user file, and a byte-identical backup of the previous executable.

Release packaging excludes installed profiles, usage databases and undo backups. Build-machine source paths are remapped in the release executable.

## Limits

Windows Server CI and Wine do not establish interactive acceptance on every Windows 10/11 desktop. Physical multi-monitor/DPI changes, alternative shells, third-party desktop organizers, ARM64 and future taskbar formats have not been established for this release. The 100 geometry combinations verify placement calculations, not universal hardware compatibility. Version **0.2.8 remains a preview**.

## 0.2.10: manual protection during long source outages

A 45-day simulated review/recovery sequence preserves a manually protected shortcut; two additional regressions cover English/Russian offline warnings and unchanged paths.

[Validation, run and limitations](research/2026-10-26-manual-absence-completion.md).
