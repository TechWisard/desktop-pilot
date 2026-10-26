# Readable launch aliases during a source outage (2026-10-26)

## Starting point and evidence

Baseline: published 0.2.12, commit `79b304a4bff84a8220486805e77da45e947d52e6`. Main CI run 40373395973 completed successfully. No open PR was present when this investigation began. Earlier profile, session, manual-absence and clock-recovery fixes are retained.

This continues the existing adaptive-shortcut research rather than repeating it. Gajos et al. experimentally linked predictive accuracy and predictability to satisfaction with adaptive interfaces: https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml . That is motivation to make a recommendation actionable, not evidence of measured Desktop Pilot productivity.

PowerToys documents deduplicating application results from several source locations: https://microsoft.github.io/PowerToys/modules/launcher/plugins/program/ . Desktop Pilot must additionally preserve complete launch identity, including arguments and working directory; a browser's personal profile must not replace its work profile.

A Flow Launcher user reported delayed availability of already indexed custom-source shortcuts after restart (issue 4484, 2026-05-21): https://github.com/Flow-Launcher/Flow.Launcher/issues/4484 . Another reported unwanted broken-shortcut dialogs during indexing (issue 3386, 2025-03-26): https://github.com/Flow-Launcher/Flow.Launcher/issues/3386 . These are first-person reports from another product, not proof of the same cause, prevalence, or timing in Desktop Pilot. Our fixtures read links without launching targets or adding resolving dialogs.

## Hypothesis and source finding

A stored source can become unreadable during a sharing lock or partial rewrite while an equivalent readable shortcut remains in a scanned folder. `refresh_apps` intentionally retains the stored source's last verified validity on inconclusive reads. However, its later deduplication skips every same-identity alias whenever that cached validity is true. As a result, a recommendation can remain blocked by the unreadable original even though the scan just read a valid alternative.

The change tracks only sources whose current read failed. A valid alias with the exact existing launch identity may replace that source while retaining the catalog ID, rule, usage, seed and note. An invalid alias cannot downgrade last verified validity. Once a readable source is chosen, later aliases do not keep changing it. A healthy source is unchanged. No source file is rewritten and no schema, configuration control, dependency or telemetry is added.

## Reproducible tests on production logic

`src/engine/alias_recovery_tests.rs` uses the actual SQLite Store, Windows link parser, refresh, preview, apply and policy command. All files are isolated under a test desktop; inert targets are never executed.

1. Deny all file sharing on a stored URL shortcut, leave two exact aliases, refresh and apply. Require one catalog identity and one managed copy. Release the lock and repeat five refresh/preview/apply cycles; require zero proposed replacements and unchanged owned path, bytes, file ID and position. Then choose Never and verify removal of only the managed copy, preserving history and every original.
2. Leave a source with a partial InternetShortcut section. Recover through its readable alias without repairing or overwriting the partial original.
3. Begin with an existing managed copy, lock its source and discover a readable alias. Require the existing copy to remain byte- and identity-identical, without recreation.
4. Repeatedly discover an earlier-sorted duplicate of a healthy source. Require no source switching or history duplication.
5. Use real .lnk files targeting the same inert executable with different launch arguments. Lock Work's source; Personal must not substitute for Work. After unlocking, placement must still have Work's arguments.

Before/after metrics are fixture outcomes: successful requested placement versus a blocked placement, catalog and managed-copy counts, unnecessary replacements across repeated reviews, and preservation of user state. They are not human task-time estimates. Expected baseline is three failing regressions and two passing boundaries. Do not count compilation or formatting errors as reproduction.

To replay the baseline, use a separate worktree at the baseline commit, copy the new test module to `src/engine/alias_recovery_tests.rs`, append `#[cfg(test)] mod alias_recovery_tests;` to `src/engine.rs`, then run `cargo test --locked alias_recovery_tests`. Run the identical tests after the implementation change, followed by the full unchanged Windows CI and native fixtures.

## Handover and limits

Complete and record candidate/PR/main CI and publication before claiming a new released version. The existing publication workflow verifies release assets and tag/commit consistency.

Not exercised: a physical Windows 11 desktop, an actual OneDrive client or remote share, prolonged network timeout, or measured task time for people. If no readable same-identity alias is found, existing conservative behavior remains; this does not solve an entire profile drive disappearing. Continue longitudinal project-switch/absence tests and real Explorer validation separately. No Windows-version matrix was added.
