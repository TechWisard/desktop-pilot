# Reimporting a user-edited quick-copy icon — 2026-10-28

## Actual starting point and external context

Baseline: published Desktop Pilot 0.2.31, `f7b3da1454033139e85a1cdd8197ca562d498d34`. Main CI succeeded, PR #33 was merged and no PR was open at inspection. This work continues the prior shortcut ownership and no-surprise-placement investigations, not the resolved identity-index change.

Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus*, CHI 2004 (https://doi.org/10.1145/985692.985704), found users valued control and stable layouts; this is design motivation, not a measured outcome here. A concrete independent report, Flow Launcher #2850 (https://github.com/Flow-Launcher/Flow.Launcher/issues/2850, July 20, 2024), describes a user changing the icon of a Windows Internet shortcut through Explorer Properties. This proves the interaction is realistic, **not** that Desktop Pilot has the other product's icon display problem.

## Source-confirmed inconsistency

`engine::imported_quick_copy` recognized managed copies by stored path or filesystem ID but accepted them as unchanged when only `App::identity` (target, arguments, working directory) matched. Icon changes in a `.url` file leave that launch identity unchanged. The Settings import could therefore report the user-edited managed copy as successfully added, while a later real `reconcile_manual` would correctly reject its changed SHA-256 bytes, release ownership and apply Never to prevent unwanted recreation. The false success is confusing after an explicit user action.

The ownership ledger already records a fingerprint for every managed quick copy. The narrow correction checks those existing saved bytes after launch identity and owner matching. An edited copy receives the already translated recovery explanation, while unrelated originals in the same batch still import. A just-renamed managed copy is matched by saved filesystem identity before reconciliation. Never readjusts the altered file, overwrites a user icon or silently changes a manual rule as part of the rejected import. No new settings, schema, dependencies, background services or telemetry.

## Failure-first production-path regression

`src/ui/import_ownership_tests.rs::changing_only_a_quick_copys_icon_cannot_be_silently_reimported` invokes the real asynchronous Settings import, Windows `.url` parser, SQLite catalog/ownership and manual reconciliation on an isolated desktop. Two sequences cover the original path and a just-renamed file. Each changes only IconFile/IconIndex, confirms identical launch identity, attempts the changed copy alongside an unrelated valid original, checks per-item feedback, preserves original and user-edited bytes, source/policy/history/state, then exercises the existing manual-release path. This is not a mock import or a fabricated user-time saving.

Before correction, the formatted regression failed on both Windows 2022 and 2025 in [CI 40636434816](https://github.com/TechWisard/desktop-pilot/actions/runs/40636434816): 254 prior tests passed, one new behavioral assertion failed because the edited copy produced **zero import failures**. An earlier authoring run [40636227130](https://github.com/TechWisard/desktop-pilot/actions/runs/40636227130) failed formatting, before the behavioral test; no product claim is based on that run.

Run the unchanged Windows 2022/2025 matrix (`cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integration, README and publication guards) on the corrected commit. Merge and publish only after green checks; separately verify tag/commit, executable and public archive hashes.

## Boundaries and next work

This checks correctness of a Windows shortcut edit, not the prevalence of that workflow, human time saved or actual Windows 11 Explorer UI at 200% DPI. Existing issue #22 remains open for large-catalog physical startup measurements, disconnected cloud folders and multiday habit changes. Other manual rules, undo, layout and profile launches must keep passing their existing tests.
