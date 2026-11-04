# Zero automatic replacements with useful empty-slot recovery — 2026-11-04

## Observed need and bounded inference

Desktop Pilot's previously saved Settings limit for replacements per review accepted only **1 through the configured slot count**. A user could not ask for automatic filling of genuinely free spaces without permitting at least one automatic displacement of an already familiar shortcut. The limitation is directly confirmed by the old `Config::validate` bound and Settings number control, not by extrapolation from a third-party product.

Independent user reports illustrate why maintaining a familiar position matters: a [November 6, 2024 Fences report](https://forums.stardock.com/532016/moving-fences-moves-single-desktop-icons-as-well-causing-chaos) describes unrelated icons moving while the user adjusts groups, and [June 12, 2024 reports](https://forums.stardock.com/528482/icons-in-fences-get-rearranged-when-desktop-is-rebooted-or-refreshed) describe unexpected changes after restart. The [2008 CHI experiment on predictability and accuracy](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) found both accuracy and predictability influence satisfaction, with accuracy stronger for some measured outcomes. These studies and reports concern *other* interfaces: they do **not** measure Desktop Pilot, establish demand for this exact control or quantify time saved.

**Hypothesis to validate with users:** a separate, explicit zero-replacement option is more useful than disabling collection or automation completely for someone with stable shortcuts and occasional new projects. It allows an empty slot to become useful, while protecting existing familiar positions until the user makes a deliberate change.

## Real-program checks

The new control reuses the existing `max_changes` configuration and selection algorithm. No new state, analytics, startup service, network request or file format is introduced.

- Validate `max_changes = 0` and carry it through an ordinary Settings merge; retain validation against slot count for nonzero values.
- Replay real `rank` / `select` with two familiar Auto shortcuts and a much higher-ranked newcomer. An automatic review must not replace either familiar entry with a zero budget.
- Repeat with one genuine vacancy: the useful newcomer may fill the vacant space because no familiar shortcut is displaced.
- Repeat with explicit `Always` / `Never` policy. The deliberate user instruction or reduced configured capacity still wins; zero does not make the interface impossible to change.
- Replay `engine::preview` and `engine::apply` on the actual SQLite Store and real local Windows `.url` files. Verify that a review performs no unnecessary file changes, source bytes and positions of retained owned shortcuts are stable, and a deliberate Never choice allows the new project into the freed slot.
- Check legacy default `max_changes=1` remains unchanged for users who never touch this option.

**Acceptance metrics:** unintended automatic replacements = 0 with the zero setting; vacancy-fill delay = one requested review; familiar-copy byte/position changes = 0 during no-op review; explicit policy commands remain effective. These are synthetic correctness measurements; do not claim real-human time savings.

## Verification boundaries

The candidate must pass both normal Windows Server 2022/2025 CI matrices: `cargo fmt --check`, locked Rust tests, strict Clippy, locked build, native fixtures, README and release guards. Any red check remains a blocker, not an excuse to weaken the pipeline. Interactive Settings visibility at 100%, 125%, 150% and 200% DPI and prolonged Windows 11 operation still require physical testing. The separate work-in-progress desktop organizer PRs #98 and #101 must not be merged incidentally.
