# One clock snapshot per confirmed learning sample — 2026-11-03

## Scenario and evidence

This continues the existing [learning clock correction](2026-11-03-learning-clock-rewind-partial.md) and [calendar recovery](2026-11-03-malformed-learning-day-rewind.md) work rather than re-running the broad HCI search. Microsoft documents Windows system time as adjustable: https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time. Gajos et al. (CHI 2008) observed that accuracy and predictability matter in adaptive user interfaces: https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml. Neither establishes how frequently this corner case occurs in Desktop Pilot or a real user time saving.

**Source-confirmed inconsistency:** `record_learning_if_permitted` sampled `now()` for the activity checkpoint, then called `Local::now()` independently to pick its day, then sampled `now()` again to mark learning complete. A midnight/time-zone adjustment or blocked SQLite transaction between those reads could assign one confirmed event inconsistent timestamps. Retrying a busy SQLite writer could also shift the claimed checkpoint even though no new foreground activity was confirmed. This is a race visible in source, not a measured field incident.

## Change and production-path acceptance

Take exactly one local time snapshot before retrying the actual SQLite writer transaction. Convert that one snapshot to an absolute timestamp and its corresponding local calendar day. Reuse the same timestamp when the learning gate becomes complete. Keep the original permission check, `Progress::observe` validation, completed-learning persistence, writer-lock retry, and settings intact.

`confirmed_learning_across_midnight_persists_one_consistent_clock_sample` uses the production `record_learning_at` transaction, SQLite `Store`, and the real `Progress` implementation with 150 confirmed synthetic two-second samples. A controlled 23:58–00:02:58 sequence must produce exactly one five-minute session, 120 recorded seconds on the first date, 180 on the second, and a completion timestamp identical to the final confirmed sample's checkpoint. The test then pauses collection and confirms a further event neither contributes activity nor changes the saved completion. The source, desktop ownership and manual rules are untouched. The event count and stored timestamps are deterministic correctness checks, **not** a field productivity benchmark.

## Boundaries and continuation

This removes inconsistent multi-read checkpoints but does not repair an arbitrary incorrect Windows system clock. Run the unchanged Windows 2022/2025 formatting, all locked Rust tests, strict Clippy, build, native integration and publication guards on this branch, and verify merged-main checks independently before any release. A real Windows 11 interactive midnight/DST transition, UI at 100–200% scaling, stalls on remote shortcuts, and multiweek user habit studies remain unverified. Keep organizer drafts #98 and #101 separate.
