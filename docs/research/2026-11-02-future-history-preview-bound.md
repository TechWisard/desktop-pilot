# Cold-preview read bound after future clock correction — 2026-11-02

## Context and observed code path

Continue [forward-clock retention](2026-10-26-forward-clock-retention.md), [cold-start confidence](2026-10-27-cold-start-confidence.md), and [future learning dates](2026-11-02-future-saved-learning-days.md); no broad search is repeated. [Microsoft system time documentation](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) describes adjustable system time. [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) motivates predictability and controllable adaptation, not a Desktop Pilot performance measurement.

At the v0.2.89 source commit `9b95469736b5bc39c9fd820de4fae7da70596caa`, `Store::days()` selects every row after the 90-day lower bound with **no future upper bound**. `model::rank_with_owned()` already rejects rows dated far beyond the current time, but only *after* SQLite has returned and decoded them into a vector. A restored profile with future-dated rows from a bad wall clock can therefore waste each Settings/preview read. This is a confirmed source-level unnecessary read and a synthetic stress scenario; no actual user's latency was measured.

## Change and production-path regression

The actual SQLite query now has the same bounded time horizon as the existing production ranker: from 90 days before the current wall time through one day after it. The one-day allowance preserves the existing small clock/time-zone correction behavior. Future records **remain on disk** and may become relevant when the clock catches up; there is no deletion or destructive migration.

`store::retention_tests::far_future_clock_rows_are_preserved_but_not_loaded_into_cold_preview` uses real `Store`/SQLite, a native-parsed `.url` shortcut with a manual Always policy, an ordinary recent activity row, 512 distant-future synthetic rows and a nearby future correction row. Before the query change `Store::days()` loads 514 rows; after the change it loads **2**. The test also invokes actual `engine::preview`, expects the rare manually protected shortcut, and verifies all 514 rows and the original file still exist. The row-count reduction is a synthetic correctness/per-work bound, not observed time saved or a guarantee of faster startup on every PC.

## Boundaries and continuation

Run unchanged Windows 2022/2025 formatting, full Rust tests, strict Clippy, build, native integration and publication guards on the candidate and then on `main`. Keep organizer drafts #98 and #101 untouched. Physical Windows 11 cold-start latency, 5,000-shortcut optional roots, disconnected network folders, GUI at 200%, and long mixed-project transitions remain acceptance work. No new setting, telemetry, network dependency, or elevated access is introduced.

## Historical clock-rewind acceptance

The initial candidate passed formatting but exposed a pre-existing test's synthetic clock mismatch: `model::selection_clock_tests::short_clock_rewind_across_midnight_keeps_managed_usage_but_not_new_candidates` used a fixed 2033-era timestamp for the ranking while calling `Store::days()` against the Windows runner's real 2026 clock. Once SQL correctly rejected far-future rows, that test lost its intentionally injected two-hour clock-rewind evidence. The production Store query is now reused by a private `days_at(at)` clock seam, and this test loads history using the *same* injected instant as `preview_loaded`. Neither the test's eligibility assertions nor its stability requirements are relaxed. This failure is a source/fixture time-domain mismatch, not a new observed user-facing regression.
