# Manual placement must mean manual control — 2026-10-27

## Starting point and evidence

Continue from public v0.2.22, main `3aa53990ecec1c5b398697f6e92285b5eb3b72ce` (Windows CI and publication passed). Existing cold-start catalog snapshot, source recovery and hand-authored ownership tests are retained. No open PR was found at initial inspection.

Primary research: Findlater and McGrenere, [CHI 2004, *A Comparison of Static, Adaptive, and Adaptable Menus*](https://doi.org/10.1145/985692.985704), compared 27 participants and found predictable adaptable controls generally preferable to movement by adaptive menus. A [PowerToys FancyZones user report #22262](https://github.com/microsoft/PowerToys/issues/22262), opened **2022-11-23**, describes manually selected layouts unexpectedly reverting after Windows sign-in. The report concerns **another application**; it only motivates testing persistence of user intent. Neither source measures Desktop Pilot's productivity.

The **confirmed defect is in our own production code**. The Settings screen explains Manual mode as “Choose apps and apply changes yourself. New usage statistics are not collected.” That sets `Config.automatic=false` and `Config.collect=false`. Nevertheless `model::select` used to process all eligible usage-ranked `Auto` candidates, filling empty slots and replacing previously familiar shortcuts. Since a user can keep old usage history when switching from Observe/Adaptive to Manual, pressing **Apply now** could cause surprising placement despite an explicitly manual mode. `rank` still calculates eligible information for explanation; it was `select` that violated the UI promise.

## Failure-first reproduction on actual code paths

On a test-only branch from the unchanged released source, attach four `model::select` scenario regressions and one real Windows COM / `.url` / `Store` SQLite / `engine::preview` / `apply` / `command` / `rollback` regression. The test-only branch [CI 40478366553](https://github.com/TechWisard/desktop-pilot/actions/runs/40478366553) passed formatting and compiled the actual unchanged production code on Windows Server 2022 and 2025. Its Windows 2022 log showed **232 passing tests and three failing assertions**:

- A Manual request with an explicitly chosen app and an old high-scoring Auto app selected **2 instead of 1**.
- Changing to Manual with one stable Auto-owned copy and a stronger challenger replaced **1 familiar shortcut instead of 0**.
- The real SQLite and desktop-copy scenario selected a learned app even though the configuration was Manual.

Two other new scenario tests (manual Never/capacity and Observe/paused Adaptive boundaries) passed. The earlier authoring run 40478180963 failed formatting, so it is **not** counted as behavioral evidence.

## Correction and meaningful sequential scenarios

`select` now preserves owned user-allowed shortcuts, explicit Always decisions and explicit capacity reductions, then stops before the score-ranked candidate insertion/replacement loop **only when both automation and collection are off**. The saved ranking remains visible for explanation, but cannot silently change the desktop in Manual mode. Observe-only still allows explicitly applying saved usage-based proposals; automatic-with-collection-paused still uses saved evidence and existing opt-in. No ranking thresholds, new settings, background services, privacy settings, file contents, history schema or Undo journal changed.

The native in-process integration scenario uses real `Store` SQLite, real COM parsing of inert `.url` files and the real `engine::apply`/rollback path: begin Manual with an Always-chosen rare app and a high-ranked frequent app; switch to Observe and explicitly apply the suggestion; switch back to Manual after another app becomes busy, apply and verify no extra changes or displacement; choose Never on the frequent app, apply, and undo the most recent desktop transaction without changing original source files or the saved prohibition. The fixtures never execute target programs and are not a real interactive Windows 11 Explorer session.

Previously written tests that specifically demonstrate **automatic/usage-guided** placement now declare Observe mode (`collect=true, automatic=false`) explicitly rather than depending on `Config::default()`, which intentionally starts in Manual. Their substantive assertions and protective checks remain intact. This keeps the Manual and Observe contracts independently testable.

The before/after numbers above are counts from deterministic selected identifiers and managed copies. They are not estimates of human task-time or click savings. Remaining work: real Windows 11 GUI inspection at 100% and 200%, longitudinal project/weekend behavior, full source-drive disappearance and the usability effect of explicit explanation under mixed profiles.

## Verification and continuation

Required before merge: `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, native integration and publication guards on both unchanged Windows CI runners. Record the candidate, main and public tag/asset results separately. The initial candidate run 40478783398 exposed nine old **test-mode fixture** assumptions after changing Manual selection; these were converted to explicit Observe mode. The next run 40479290097 passed unit tests but Clippy found one unnecessary test-only `clone`; corrected without disabling lints. Do not describe these earlier runs as fully green. The final verified run and release metadata must be checked independently.
