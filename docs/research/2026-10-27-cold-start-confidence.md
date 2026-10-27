# Cold-start confidence and stable replacements — 2026-10-27

## Evidence and starting state

The published baseline is v0.2.18 (`c78b6b8cf55b4501037c96d1acc4f32418d03df4`). A separate work branch `fix/cold-start-confidence-20261027` introduced a 3.0 automatic eligibility score. Its [Windows CI run 40433840999](https://github.com/TechWisard/desktop-pilot/actions/runs/40433840999) passed 216 Rust tests but failed an unchanged actual Shell-shortcut test: a recorded ten-minute session was incorrectly excluded. This branch preserves that work, without rewriting the original branch.

Primary evidence: [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) found that adaptive moving menus could be slower and users often preferred adjustable, predictable interfaces (27 participants). [PowerToys Workspaces user report #34823](https://github.com/microsoft/PowerToys/issues/34823) describes a slow failed launch and unexpected placement on multiple monitors; that is an analogous report, **not** an observed Desktop Pilot defect.

## Production-code behavior

The real `rank` score for one confirmed 12-second visit is about 2.07, one 300-second session about 2.72, and a 600-second session about 2.96 at zero age. A single 3.0 threshold used for both *new placement* and *replacement* unintentionally blocked meaningful 600-second work. The tested revision uses 2.5 for a first automatic candidate but preserves the original absolute **3.0-point** replacement margin and existing configured multiplier. Manual Always/Never, taskbar pins, existing owned copies and Undo are not gated by this new threshold.

## Reproduction and synthetic measures

The existing nine cold-start tests and two added tests run the production `rank` and `select` logic, plus real `Store`, preview/apply/reconciliation and Undo. A recent five-minute recorded session is eligible; an isolated three-day-old session is not. A challenger only 2.8 score-points stronger than an existing 20-point shortcut does not displace it; a 3.5-point stronger challenger can, subject to the regular rules. The unchanged Shell shortcut integration test must again pass. The one-short-visit test continues to refuse an accidental new placement. These are reproducible correctness checks, **not** measured human time savings or evidence that 2.5 is a population-optimal threshold.

## Required verification and continuation

Before release, run unmodified full Windows 2022 and 2025 CI: cargo formatting, locked Rust tests, strict Clippy, build, native integrations and publication guards. Verify candidate, main tag, Windows executable and public archives independently. Remain to study multi-week changing projects, weekend effects, real Windows 11 GUI at 200%, cloud-disconnected profiles and quantitative real-user outcomes.

## Completed candidate verification

[Windows checks run 40436887407](https://github.com/TechWisard/desktop-pilot/actions/runs/40436887407) completed successfully for both Windows Server 2022 and 2025: 219 Rust unit/integration tests (zero failures), 28 native integration checks per runner, nine publication guards per runner, `cargo fmt --check`, strict `cargo clippy --locked --all-targets -- -D warnings` and `cargo build --locked`. A prior run failed strict Clippy on four new test-only clones; these were corrected without relaxing assertions or lint rules. Earlier formatting failure was corrected as well. The previously failing actual packaged-Shell shortcut regression passes under the separate initial-placement threshold.

These runner checks are synthetic/isolated Windows tests, not a real user desktop session. Merging, final main CI, the version-tagged build and public assets require independent verification.
