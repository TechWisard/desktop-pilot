# Reimporting quick copies without losing manual control (2026-10-27)

## Starting state and evidence

Continue from 0.2.16, main `feb1041d837adc3eeb823515f92f6b0bb01c0e93`; its main/release CI 40411469234 succeeded. No open PR was present at inspection. Existing source-alias recovery, profile continuity, history retention and heartbeat work are retained. This is not another Windows compatibility matrix.

The earlier adaptive-shortcut investigations already cover the literature. Findlater and McGrenere's CHI 2004 study compared static, adaptive and adaptable menus (https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/); Gajos et al. studied accuracy and predictability (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). These motivate predictable manual control, not a claim that this patch has measured human productivity.

A primary PowerToys report describes an app-search path that could not be pinned because search items and top-level items had different representations: https://github.com/microsoft/PowerToys/issues/45848 (2026-02-28). The report includes the implementation's filtering of already-pinned application IDs. This is an analogous interaction/identity failure in another tool, not evidence of the same root cause or prevalence here. Flow Launcher issue 1682 (https://github.com/Flow-Launcher/Flow.Launcher/issues/1682, 2022-12-18) describes editing a second shortcut continuing to operate on the first selection. Again, the engineering inference is to test repeated manual actions against persistent identity; neither report proves a Desktop Pilot defect.

## Source finding and hypothesis

Settings accepts dropped/selected .lnk/.url files. The import path preserved the existing app ID and rules but unconditionally replaced its source with the chosen file. A user can naturally choose a quick copy already visible on the desktop. That promotes a managed copy to the catalog's original source. The next production reconciliation then releases that file and sets Never, because originals must never be managed. Undo also refuses to modify a path now recorded as an original. Thus an apparently harmless repeat import can undo a manual rule and make Undo unavailable.

A just-renamed quick copy needs file-identity matching too: its new path may not yet have been reconciled. Import must also serialize with placement/undo and refuse unfinished transactions, otherwise it can change the original-source set while recovery depends on it.

## Narrow correction

Import takes the existing profile-scoped changes lock before reading links and checks the existing unfinished-operation guard. In the SQLite import transaction, a path or saved filesystem identity belonging to a managed copy resolves to the existing catalog app without rewriting it. The complete launch identity must still match; an edited target is rejected with a localized explanation while unrelated valid files in the same batch are imported. No shortcut files are rewritten, new app controls are added, or ranking weights are changed. Ordinary duplicate originals can still refresh a source, and different arguments remain separate profiles.

## Production-path regressions

`src/ui/import_ownership_tests.rs` invokes the real asynchronous `Settings::add_shortcuts`, its COM link reader and SQLite transaction, then actual refresh/reconciliation/preview/apply/rollback. It does not substitute an import model.

1. Repeat quick-copy import and review five times. Check stable source, Always, note, usage, bytes, file identity and position, one catalog identity, no proposed replacements and functioning Undo.
2. Import a quick copy and immediately undo; original bytes must survive while the managed copy is removed.
3. Rename a quick copy and import it before reconciliation; preserve its original source, accept its new name during reconciliation, then obey Never.
4. Temporarily remove the original, import its quick copy, restore the original, choose Never, import again and apply. Do not disguise an outage or defeat the explicit prohibition.
5. Edit a quick copy to a different launch target and import it together with an unrelated valid original. Report the changed copy, accept the unrelated file and preserve user-edited bytes.
6. Mark a real transaction unfinished, attempt a mixed import, compare the unchanged catalog/state/history and recover through real Undo.
7. Reimport an ordinary duplicate original; allow source refresh and keep Undo working.
8. Import the original and managed copy in both orders; obtain the same catalog and rules.
9. Import real Work/Personal .lnk profiles of one inert executable; retain distinct launch arguments and catalog IDs. The fixture never runs that executable.

A tenth regression checks the new recovery explanation in both languages. Repeat count and unchanged selection/position are synthetic correctness metrics, not estimates of people's time savings. No on-screen input capture, physical Explorer session, real cloud-sync client or long-horizon habit study is claimed.

## Reproduction and continuation

Preparation runs the nine UI regressions on the unchanged production code first (only the test module is wired in), then applies the checked patch and runs formatting, the full suite, warnings-denied Clippy, build, native integration and publication guards. Actual results and immutable input hashes are recorded in `2026-10-27-import-ownership-validation.json`; a preparation pass is not a release certificate. Full PR/main CI and tag/assets must pass independently.

Reproduce the baseline in a separate worktree at the baseline commit: copy `src/ui/import_ownership_tests.rs`, append `#[cfg(test)] mod import_ownership_tests;` to ui.rs, and run `cargo test --locked ui::import_ownership_tests -- --test-threads=1`. On the corrected tree run the same tests plus the unchanged checks workflow.

Remaining: physical Windows 11 interaction/visual review, complete disappearance of the profile drive, long network stalls and broader longitudinal project-change/adaptation-delay experiments. The patch makes existing manual decisions and Undo durable through a common repeat action; it does not establish globally optimal recommendations.
