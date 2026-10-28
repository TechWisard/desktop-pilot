# Combined project-change and manual-reversal acceptance (2026-10-29)

## Scope

The latest verified public release at the start of this work was 0.2.43 (main `85b19f726ae7754b335dedb4da786283221fb2b1`, successful [Windows CI](https://github.com/TechWisard/desktop-pilot/actions/runs/40767664013)). No open PR was present. The project is already public and its standard Windows Actions pipeline works; no separate finite-checks repository, private code transfer, payment or CI bypass is needed.

Continue existing research in `docs/research/2026-10-26-adaptive-shortcuts.md` and [the ongoing large-catalog issue](https://github.com/TechWisard/desktop-pilot/issues/22). Research by Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) motivates predictable manual control; it is not evidence that any particular Desktop Pilot behavior saves time. This stage adds a **combined acceptance trace** rather than repeating external searches.

## Actual production-path sequence

`src/model/cold_start_tests.rs` uses native-parsed inert `.url` files, a temporary real SQLite `Store`, actual `engine::preview`, `apply`, `automatic_review`, `refresh_apps` and `rollback`. It checks an explicit Always/rare shortcut, a historically high-use former project, a new project gradually gaining usage over four historical dates, a Never exclusion, repeated no-new-data reviews, quiet-time review gating, deliberate Undo, a temporarily absent original and user-requested explicit Apply during paused automation.

The controlled history records are inserted in the real daily schema and are all in the past relative to the test clock. This is **not** a real multiweek recording of a person or a model of network delays. Assertions cover proposed additions/removals, stable ownership/path/position, rejected app never added, permitted one-slot replacement, no movement from inactivity alone, actual file-level rollback, paused automatic replay and exact preservation of all originals.

## Boundaries

This is acceptance coverage, not a product algorithm change. If the sequence passes, do not manufacture a new release: merging a test-only PR leaves 0.2.43 the latest binary. If it fails, diagnose the production path and make the smallest safe correction with red/green evidence before releasing. Keep Windows formatting, locked Rust tests, strict Clippy, native integrations, existing release checks and CI unchanged. Physical Windows 11/200% DPI, real network outages and observed human adaptation remain unmeasured.
