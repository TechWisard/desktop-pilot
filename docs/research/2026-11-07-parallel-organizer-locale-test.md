# Organizer bulk-action acceptance under parallel localization tests — 2026-11-07

## Observed failure, not a user-language regression

The integrated organizer's Windows Server 2022 [CI run 42339012780](https://github.com/TechWisard/desktop-pilot/actions/runs/42339012780) had **515 passing and 1 failing Rust test** (three deliberately ignored). The failed `ui::organization::tests::one_bulk_choice_changes_the_entire_real_plan_even_with_hidden_rows` reported `Missing visible organizer control 'Deselect all'`. The same rendered Egui frame explicitly contained the working Russian button `Снять все отметки`, the correct `Отметить все` control and a two-item plan; English and Russian strings appeared in the same test frame. The same source commit had green checks on Windows 2025 and Windows 11 ARM.

The test intentionally wrote to the process-wide `i18n::set("en")`, but other Rust tests execute concurrently and may modify that same global `AtomicBool`. This test was selecting a real widget by English display text rather than by its stable action semantics. The captured failure is therefore evidence of a race in the **test harness's global-language assumption**; it is not evidence that a user's single Settings process unexpectedly changes languages.

## Narrow correction

The production organizer UI and local files are unchanged. The visual-interaction test no longer changes process-wide language. Instead, it identifies the same real clickable widget by either of its two valid rendered labels: `Deselect all` or `Снять все отметки`, and `Select all` or `Отметить все`. It still uses rendered Egui shapes, on-screen geometry, actual pointer events and the production Settings/SQLite-backed plan, rather than calling the action handler directly.

The unchanged behavioral assertions cover a filtered plan in which both rows are hidden, clearing and reselecting the **entire** plan, a partial one-row filter, Apply with zero chosen moves, unchanged shortcut bytes and folder layout, absence of an undo journal, and the persisted keep-on-desktop choice after re-analysis. Accepting a valid localized label does not weaken any of those checks.

## Acceptance

Require the ordinary public Windows workflow with `cargo fmt --check`, all locked Rust tests, strict Clippy, build, native integration and Windows Explorer/UI acceptance on all three runner types. This regression removes a language-dependent false failure without suppressing tests or altering the release gate. Longer-term interactive UX feedback still requires a Windows desktop human review; a green synthetic Egui test is not a user productivity study.
