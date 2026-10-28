# Complete discovery of large shortcut folders (2026-10-28)

## Starting point and existing evidence

Baseline: published Desktop Pilot 0.2.35, main `84a3df54973321ceec1d30bef1d7282d14ee2439`, successful Windows CI 40691817819. No open PRs at inspection. Continue existing cold-start investigation [#22](https://github.com/TechWisard/desktop-pilot/issues/22), the 100/1,000/5,000 saved-catalog search fixtures and the 0.2.35 relevance improvement. This is a separate **discovery completeness** concern, not another Windows compatibility exercise.

Primary research on predictable adaptive interfaces remains [Gajos et al. 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) and [Findlater and McGrenere 2004](https://doi.org/10.1145/985692.985704); those studies do not establish Desktop Pilot's limit or performance. A [Flow Launcher reporter](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484) described long delays for custom shortcuts in May 2026; that issue was closed June 8 after plugin updates appeared to solve it. It motivates large-catalog acceptance testing, not a claim that another product shares our defect.

## Confirmed source-level inconsistency

At 0.2.35, `engine::refresh_apps_once::visit` stopped collecting after 3,000 `.lnk`/`.url` entries across the scanned roots, despite issue #22 explicitly requiring 5,000-shortcut scenarios and the saved SQLite/search path supporting 5,000. The code emitted a scan-limit warning, but the remaining original shortcuts were never put into the newly discovered catalog. This is an actual source-derived ceiling, not a measured network slowdown. The relative coverage on an all-new 5,000-link folder was at most 3,000 / 5,000 = 60%.

## Reproduction and narrow correction

`src/engine/discovery_limit_tests.rs` creates 5,000 inert `.url` files in an isolated Windows test desktop and runs the **real** `native::read_link`, `engine::refresh_apps` and SQLite `Store` catalog paths. On the existing implementation, its completeness assertion expects 5,000 entries and fails at the previous 3,000 limit. The corrected limit is 6,000, keeping a hard upper bound and the existing warning/retention mechanism. The test then explicitly sets a rare app to Always and another to Never, rescans the same 5,000 files, and confirms catalog size, rules and untouched original bytes.

No automatic placement is inferred from these catalog entries; no targets are executed. The implementation changes the discovery bound and its localized warning, not ranking, COM parsing, SQLite schema, desktop ownership, undo or collection. A bilingual regression checks both the new 6,000-link warning and the historic 3,000-link message still displayed in old logs. The separate scan worker continues to protect interactive command handling from slow periodic scans.

## Evidence and continuation

Failure-first reproduction branch: `test/large-discovery-limit-red-20261028`, test wired into unchanged baseline production code. Its first authoring CI 40694087691 failed rustfmt before running tests and does **not** count as behavioral evidence; the formatting-only correction is `cd2fbad3c612a97db4f607de590ae8560029f7a3`, with CI [40694203161](https://github.com/TechWisard/desktop-pilot/actions/runs/40694203161). After clean rustfmt, that baseline run compiled the real app and **failed precisely at 3,000 vs expected 5,000**, with 265 passing tests and one failure on each Windows runner. The corrected large-catalog build passed its first candidate checks on [40694751768](https://github.com/TechWisard/desktop-pilot/actions/runs/40694751768); bilingual warning coverage was then added and requires its own unchanged CI. The final passing candidate/PR and main release checks must be confirmed separately before publication.

Still open: measure actual physical Windows 11 first-search and first-action times at 100%/200% DPI, inspect behavior with network/cloud roots that block or disappear, and quantify several-week work/personal habit changes with false placements and unwanted movement. Expanding discovery changes **completeness**, not proven human productivity. Never disable file safety or publication guards to obtain a passing result.
