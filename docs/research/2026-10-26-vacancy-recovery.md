# Vacancy recovery without replacement churn — 2026-10-26

## Evidence and scope

- Published evidence: Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004), found that participants generally preferred user-controlled menus and that automatically adapted menus were not consistently faster. https://doi.org/10.1145/985692.985704
- Published evidence: Todi et al., *Adapting User Interfaces with Model-based Reinforcement Learning* (2021), treat surprise and relearning as adaptation costs and demonstrate the value of conservative policies. https://arxiv.org/abs/2103.06807
- Hypothesis, not measured user behavior: leaving usable shortcut slots empty for multiple review intervals reduces practical utility; filling a vacancy is less disruptive than replacing a known shortcut.
- No user study of Desktop Pilot was performed. Below are source inspection and deterministic real-selection fixtures, not measured time savings.

## Confirmed source defect

The existing model::select routine incremented one counter for both filling a vacant slot and replacing an already-owned shortcut. With slots = 4, max_changes = 1, and one existing manually preserved shortcut, a non-initial review could fill only one of three vacant positions, even with several valid ranked candidates. The next automatic review normally waits seven days. The existing max_changes setting is presented to users as a limit on *replacements*, not the number of vacant places that can be filled.

Correction: first carry over eligible existing shortcuts and all protected manual placements, then fill vacant places in descending score order without consuming the replacement budget. Only displacing a non-manual shortcut consumes the budget, and requires the original 1.5x and three-point score advantage. The existing engine::preview capacity preflight still refuses overcommitted manual choices. The patch makes no filesystem, persistence, telemetry, or default-setting changes.

## Reproducible real-policy tests

1. Vacancy recovery: one protected shortcut and three vacancies; three eligible candidates, max_changes = 1. model::select(initial = false) must preserve the protected shortcut and fill all three spaces in one review.
2. Mixed recovery and replacement: two owned shortcuts, one protected; two vacancies and four challengers; max_changes = 1. Fill both vacant spaces, replace only the weak owned shortcut, and leave the further challenger out.
3. Existing regressions: at_most_one_replacement, no_churn_below_threshold, manual_position_is_retained, and manual_choice_beats_frequency_and_pin still test the same production selection function.

## Verification criteria and limits

Run cargo fmt --check, cargo test --locked, cargo clippy --locked --all-targets -- -D warnings, cargo build --locked on Windows CI. Check existing native file ownership and rollback tests where the workflow permits them. A green CI run demonstrates these fixtures and detected regressions, not quantified benefits for real users or full Explorer behavior.

## Follow-up scenarios

- Temporary drive disconnect versus permanent uninstall currently appear alike in filesystem discovery. Research a reversible grace policy before altering removal semantics.
- Simulate review-interval changes, multiple disappearing shortcuts, protected custom positions and profile changes across realistic sequences on actual production selection code.
- Compare vacancy duration and ordinary replacement count against baseline in reproducible replay fixtures, without collecting real user telemetry.
