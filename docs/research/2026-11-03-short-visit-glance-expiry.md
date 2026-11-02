# Incidental focus glances must not renew partial usage — 2026-11-03

## User scenario and evidence

This is a continuation of [short foreground visits](2026-10-26-short-foreground-visits.md) and [adaptive-shortcut research](2026-10-26-adaptive-shortcuts.md), not a new claim about measured user behavior. A person spends several seconds in an editor, then briefly visits its window once every nineteen minutes while mostly working elsewhere. The brief return's **first** sampling tick is ambiguous and is intentionally excluded from observed usage. A later genuine confirmed visit should count only if the last confirmed engagement is still inside the configured twenty-minute in-memory window.

The original [CHI 2008 study on predictability and accuracy](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) shows that both matter to adaptive-interface satisfaction; the [CHI 2004 adaptive versus adaptable menus study](https://doi.org/10.1145/985692.985704) supports preserving explicit user control. These results **do not** validate Desktop Pilot's twenty-minute cutoff, establish this scenario's frequency, or quantify time savings. The following is a source-confirmed behavioral defect, tested with synthetic event schedules.

## Reproduction on production logic

`UsageAccumulator::sample` already ignored the first tick after switching apps, but previously refreshed `UsageWindow.last_seen` **before** checking whether the tick was confirmed. An ambiguous one-tick glance could therefore keep an earlier eight-second partial window alive for hours. A later genuine two-second confirmed sample could make that stale evidence reach the ten-second persistence threshold, creating a usage record and session even though the original confirmed engagement had expired.

The regression `incidental_focus_glances_cannot_keep_old_partial_usage_alive` runs the production accumulator and real `Store` SQLite observation path: five two-second editor ticks (first ignored, eight confirmed seconds); four isolated glances nineteen minutes apart, with other foreground ticks between them; and one later confirmed tick. **Before:** one incorrect ten-second usage record and session. **After:** no completed usage record or session; the partial window has expired.

The companion `confirmed_return_within_short_visit_window_keeps_real_usage` confirms that a return after less than twenty minutes *does* complete those ten seconds and commits a session through `record_usage` and SQLite. Existing multi-app, long-session, pause/resume, sleep, memory-bound and manual-policy tests remain in place. The configured timeout, SQLite format, ranking thresholds, default configuration, and desktop layout are unchanged.

## Change and safety

Only newly created windows get an initial `last_seen` from their ambiguous first tick. For an existing partial window, its timestamp is extended only on a confirmed consecutive foreground sample. This avoids attributing unverified focus changes as time spent working and prevents stale fragments from manufacturing a shortcut recommendation. Nothing about keyboard contents, user files, manual rules, pin locations, rollback, telemetry or network access changes.

## Verification and remaining uncertainty

Run unchanged `cargo fmt --check`, `cargo test --locked`, strict `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, native Windows integration fixtures and release guards on the existing public Windows CI. Tests replay synthetic timing through actual application code and SQLite, **not** observed human work. Actual input-hook delivery, Windows 11 interactive behavior, intermittent wakeups and the user-specific optimal timeout remain unmeasured. Keep this fix separate from unfinished desktop-organizer PRs #98/#101.
