# Mixed-project session continuity after a corrected Windows clock — 2026-10-31

## Baseline and reason to test

At the beginning of this investigation, public main was \`e31f6e856ac81f5a14725340542980822e749ab7\` and the latest binary was 0.2.69. No open PRs were present; standard public Windows Actions was operating. There is no need to create a parallel public test repository or change visibility.

Continue the previously verified background work rather than repeating it: [clock-correction recovery](2026-10-26-clock-correction-recovery.md), [session-checkpoint retention](2026-10-27-session-checkpoint-retention.md), and the [adaptive UI evidence](2026-10-26-adaptive-shortcuts.md). The Microsoft [System Time documentation](https://learn.microsoft.com/en-us/windows/win32/sysinfo/system-time) distinguishes adjustable civil time from elapsed-time measurement. The Findlater/Gajos studies cited above motivate avoiding unstable, falsely inferred recommendations. These are *design premises*, not measurements of Desktop Pilot users.

An initial hypothesis proposed that a four-hour backward clock correction must not create any new session on the first independently confirmed visit. Source comparison **rejected this hypothesis**: \`engine::record_usage\` deliberately counts exactly one confirmed recovery session for a checkpoint at least 20 minutes in the future. Existing \`corrected_backwards_clock_restores_new_usage_sessions_and_checkpoints\` proves that behavior was intentionally adopted to prevent freezing learning after the Windows clock is corrected while the monitor is stopped. Suppressing the first recovery session would regress existing protection; no change to production session policy was warranted.

## Additional mixed-project acceptance scenario

\`src/engine/session_retention_tests.rs::backward_clock_step_preserves_one_fresh_session_and_unrelated_history\` calls the **actual** \`record_usage\` SQLite transaction with two saved app checkpoints and synthetic, controlled timestamps:

1. App A has a checkpoint four hours ahead of corrected system time. App B has a ten-minute-old checkpoint.
2. Confirmed return to A after correction: A earns **one** session; B's SQLite checkpoint is untouched.
3. Confirmed brief return to B thirty seconds later: it earns **zero new** sessions (less than twenty minutes since its genuine previous activity), and A's checkpoint is untouched.
4. Return to A ten minutes later: still one A session. Return after a further 21 minutes: second A session, correctly respecting the ordinary gap.
5. The final SQLite checkpoints must reflect exactly those visits, with no fabricated B session.

The fixture exercises real \`Store\`, persisted \`app_session_times\` and \`days\`, and production \`record_usage\`; it does not simulate the Windows clock, desktop UI or input hook. All observations are synthetic; they are **not evidence of real-world time savings**. This extends the older one-app rewind test by combining independent project histories and checking that the correction for one project does not falsify another's rank inputs.

## Acceptance and continuation

Run unchanged \`cargo fmt --check\`, \`cargo test --locked\`, \`cargo clippy --locked --all-targets -- -D warnings\`, \`cargo build --locked\`, native integration checks and publication guards on the existing Windows Server 2022/2025 runners. Verify the branch's exact head and the actual CI result before merging; verify main again afterward. A passing acceptance test with no production behavior change **does not justify another binary release**. Do not interpret an initially incorrect failing test expectation as a product defect.

Still outstanding: instrumenting first useful search under real disconnected profile drives; prolonged multi-project human workflows; Windows 11 physical UI at 200% DPI; and task-level before/after measurements of recommendation utility and layout churn. Preserve local-only operation, originals, explicit rules and Undo.
