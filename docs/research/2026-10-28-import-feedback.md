# Accurate import feedback without extra desktop changes — 2026-10-28

## Starting state and evidence

Baseline: published v0.2.32 at commit `c42ac37fd7240a8051e7302db99039002b2e24e2`. Main Windows CI succeeded and there were no open PRs at inspection. This is a small human-control correction continuing the import ownership fixes in 0.2.17, live catalog import in 0.2.29 and edited-copy protection in 0.2.32. Earlier documentation already establishes the importance of predictable behavior: Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) compared static, adaptive and user-adaptable interfaces; Gajos et al. (CHI 2008, https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) found better accuracy and predictability improved satisfaction. This motivates honest feedback but does not prove a user-time benefit.

An independent Flow Launcher user reported that unsolicited warnings about previously removed/broken shortcuts were disruptive (issue #3690, 2025-06-09: https://github.com/Flow-Launcher/Flow.Launcher/issues/3690). That report involves a different program and different defect. The relevant engineering hypothesis is that inaccurate or unnecessary notices weaken trust in shortcut management.

## Source-confirmed UI inconsistency

The real asynchronous `Settings::add_shortcuts` returns a successfully selected existing catalog identity even when the import is intentionally a no-op. `Settings::poll_jobs` previously displayed "Shortcuts added: 1" and a "Recently added" filter for an unchanged owned quick copy despite not inserting/updating any app or incrementing the catalog epoch. A mixture of one actual new URL and an unchanged quick copy reported "Shortcuts added: 2". Importing a second original with the same launch identity refreshed the existing source rather than creating a second app, yet also reported an addition. These messages contradict the saved state, although shortcut ownership itself stays correct.

## Narrow correction and acceptance

Retain the production importer, profile-scoped changes lock, real native link parser, guarded ownership/undo and unchanged SQLite writes. Compare only IDs touched by this import against their initial saved values, after deduplicating by catalog identity. A no-op reports "Already in list" with an informational notice; mixed batches distinguish actual additions, source updates and previously listed entries. The post-import filter is now named "Last import", and its highlighted item is "Selected", so a pre-existing app is not mislabeled newly added. Nothing is deleted and no new setting, database schema, dependency, service or telemetry is introduced.

New production-path tests in `src/ui/import_ownership_tests.rs` execute the real asynchronous Windows .url import, SQLite Store, Settings completion/notice and Undo. Scenarios: (1) repeat import of an owned quick copy, with no changed source/rules/history; (2) one new URL plus one existing owned copy, checking counts, source bytes, desktop ownership and Undo; (3) a duplicate original that changes the saved source for the same launch ID, checking "Updated" not "Added". Preexisting nine ownership regressions and the full Windows integration suite remain mandatory. The previous code incorrectly announced a new shortcut; the comparison evaluates misleading-notice count, not measured human time savings.

## Verification boundaries and next steps

The complete CI on Windows Server 2022/2025 must pass formatting, locked Rust tests, strict Clippy, build, 28 native checks, README and publication guards before merge/release. Publishing additionally requires independent tag/main/asset hash and source tree checks. These fixtures do not simulate actual Windows 11 Explorer dialogs, 200% visual rendering or time saved by real people. Continue issue #22 for 100/1,000/5,000-entry physical cold-start timing, unavailable network roots, multiday project switches, unusual profiles and long-run placement stability.
