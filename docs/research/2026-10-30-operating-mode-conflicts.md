# Keep concurrent operating-mode decisions atomic (2026-10-30)

## Source and user impact

Baseline: Desktop Pilot 0.2.62 at `d9fc42f60f5a61178a8ea90783fc3465b39e243f`, followed by test-only PR #67 at `6f89f926ed88494e7da878f56486561c4af148df`. There were no open PRs at inspection. The main repository is already public and runs its normal Windows 2022/2025 CI; no substitute test repository is necessary.

Primary UI research: Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus*, CHI 2004, https://doi.org/10.1145/985692.985704. In their 27-participant menu study, users generally preferred direct adaptable control. It motivates prioritizing reliable explicit choices; it is not a Desktop Pilot customer report or evidence of task-time savings. Continue `docs/research/2026-10-26-adaptive-shortcuts.md` and the existing project-change/Undo fixtures, rather than repeating older searches.

Source-confirmed problem: the UI offers Manual, Observe and Adaptive as a *single choice*, represented by `Config.automatic` and `Config.collect`. The real `Settings::save` sends a draft and an edit baseline to `merge_config`, which previously merged each JSON field independently. When one editor chose Manual while another process chose Adaptive, both from Observe, the merge could silently accept the hybrid: automatic changes paused but observation still on. The user had not selected that outcome. Another interleaving of Adaptive collection pause and Observe could synthesize Manual. A concurrent explicit Turn off could be acknowledged as a successful stale Observe save even though Manual remained active. Validation accepted all these combinations.

## Failure-first evidence

The earlier unmerged test-only branch `fix/atomic-operating-mode-merge-20261029` had already demonstrated two mode failures in [Windows run 40917727730](https://github.com/TechWisard/desktop-pilot/actions/runs/40917727730). This continuation started from the *actual newer main* and added the pending test module again rather than force-merging 31 older commits.

At current baseline, the tests first required formatting and a corrected usage comparison; those intermediate CI runs are setup failures, not behavioral evidence. The final current baseline [Windows run 41009758725](https://github.com/TechWisard/desktop-pilot/actions/runs/41009758725) passed Rust formatting and reached `cargo test --locked` on **both** Windows Server 2022 and 2025. Exactly 312 tests passed and four new acceptance tests failed on each runner:
- Competing Manual and Adaptive decisions from one Observe baseline must not synthesize another mode.
- A collection pause and concurrent mode switch must not become Manual.
- An actual asynchronous Settings/SQLite save must reject a competing mode, preserving app rule, history, original bytes and Undo.
- An explicit Turn off cannot be reported as a successful stale Observe-mode save.

These are deterministic production-path races, not reported field frequencies or physical user timing.

## Correction and coverage

Treat `(automatic, collect)` as one atomic user-visible mode during a **concurrent three-way merge**: if the user draft and persisted state both changed the pair from the original baseline and disagree with one another, reject before writes or Windows startup changes. Return an actionable bilingual conflict message. If only the saved state changed, or only the draft changed, preserve the existing fieldwise merging of unrelated settings such as slots; if both select the same mode, accept it. Advanced collection pause under Adaptive remains representable. The independent `learning_enabled` option, user rules, shortcut data, history, ownership and Undo are untouched.

The four regressions live in `src/ui/mode_merge_tests.rs`; the real Settings path uses isolated temporary profiles, native URL shortcuts and a SQLite transaction. This change adds no dependencies, persistent index, background service, telemetry, or Windows compatibility matrix.

## Remaining gates and acceptance work

Require fresh ordinary push and PR checks on both Windows runners: locked Rust suite, native integrations, publication guards, formatting, strict Clippy, locked build, then release executable tests and SHA-256/tag/main verification. Do not claim a released binary until these finish. Continue separate issues #22 (physical Windows 11/200% DPI, first usable result, network outage and long-lived projects) and #56 (real input-hook callback and CPU measurements) without conflating their evidence with this logic repair.
