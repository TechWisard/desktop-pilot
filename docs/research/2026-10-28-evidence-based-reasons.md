# Explain actual recommendation evidence (2026-10-28)

## Baseline and observed inconsistency

Baseline: published Desktop Pilot 0.2.42 at `938db751de3f5fc6b7e021e7983dfe675d4d2d68`, with successful Windows 2022/2025 release checks (run 40745008727) and no open PR at the start. The existing public repository already has a functioning CI path, so no separate testing repository or proprietary-code transport is needed.

The production `model::rank` reason for every eligible automatic candidate was "Повторяемое использование; есть польза прямого входа" ("Repeated use; eligible for a direct shortcut"). However the same production code deliberately allows eligibility from (a) a saved historical priority with **zero** observed days and (b) a **single** qualifying focused session. The explanation therefore asserted repeated behavior that was not observed. It appears in the actual Apps details and Home "Why" section through `i18n::runtime`. This is a **source-confirmed accuracy defect**, distinct from any claim of lost time or an incorrect ranking decision.

Existing evidence, not newly extrapolated benchmarks:

- Findlater & McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus*, CHI 2004, DOI https://doi.org/10.1145/985692.985704: a 27-participant comparison found that many users preferred direct control and static/adaptable menus could outperform shifting adaptive ones. This motivates protecting choices, not a numerical estimate for this product.
- Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces*, CHI 2008, https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml: higher accuracy and predictability improved satisfaction in their experimental interfaces. Their measured effects do **not** establish that this wording change improves Desktop Pilot usage.
- Prior in-repository research: `docs/research/2026-10-26-adaptive-shortcuts.md` and `docs/research/2026-10-27-cold-start-confidence.md`. No repeat survey or fabricated field observations.

## Reproduction on the production logic

`src/model/evidence_reason_tests.rs` creates an isolated, real SQLite `Store` with seven Windows native-parsed, inert `.url` shortcuts. It records activity through the actual `Store::observe` for a long uninterrupted session, several shorter same-day sessions, and inserts a second distinct daily record through the real `days` schema. It also covers an eligible app with only a historical seed, a weak incidental session, explicit Always and explicit Never.

The test reads `engine::preview` and compares explanations without rebuilding a separate ranking model. It requires exactly five eligible additions in the controlled empty-desktop preview, unchanged saved usage, no moved shortcut files, and no mutation of the `State`. The real GUI reads the same `Ranked.reason` field. The new translation test checks exact round-trip translations for all five evidence explanations.

The first Windows branch run, https://github.com/TechWisard/desktop-pilot/actions/runs/40765777217, compiled the unchanged released ranking code with the new test. On Windows 2022, 281 other tests passed and the new regression failed at an eligible, **unobserved** historical-priority app: actual "Repeated use" versus expected "Saved historical priority; little recent activity has been observed". The check is a semantic failure in `rank`, not a build/format failure. Windows 2025 results are recorded in the linked run.

## Correction boundaries

Only the reason text for eligible automatic candidates changes. Branch on evidence actually stored in production: distinct active days, session count, active seconds, or a saved seed with little recent observation. Use conditional language: a quick shortcut **may** help; do not assert measured time savings. Preserve the fallback for atypical historical records.

Selection scores, thresholds, eligibility, candidate ordering, manually pinned applications, taskbar exclusions, user preferences, source files, SQLite schema, undo and automation cadence stay unchanged. `i18n` maintains both languages and retains the older reason mapping for historical compatibility. No external service, settings, logging, telemetry or dependency is introduced.

## Verification and acceptance

The new actual-Store regression and existing cold-start, long-session, manual-mode, offline, duplicate-profile, undo, UI layout, native Windows integration and release checks must pass in unchanged GitHub Actions on Windows 2022 and 2025 before merge. Relevant commands:

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
./scripts/test-native.ps1
./scripts/test-release-workflow.ps1
python scripts/test-readmes.py
```

The test is a fixed scenario, not a user study. It does not prove real-world speedup, comprehension, display latency, or week-long behavior. Continue issue #22 for physical Windows 11 / 200% DPI input-to-render, actual cold-start timing, stalled network profile drives and multiweek habit/project shifts. Do not close it based on textual accuracy alone.
