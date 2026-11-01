# Preserve queued manual choice order under transient SQLite contention — 2026-11-01

## Evidence and scenario

Continue the earlier [command claim contention](2026-11-01-command-claim-contention.md) and [terminal outcomes](2026-10-30-terminal-command-outcomes.md) investigations without repeating the literature search. SQLite documents transient [BUSY and LOCKED conditions](https://www.sqlite.org/rescode.html#busy). The established [CHI 2004 comparison of static, adaptive and adaptable menus](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) favors predictable user control; it does not measure Desktop Pilot's productivity.

**Confirmed by source inspection:** after the previous writer-contention fix, `monitor` iterated `Store::pending()` in ID order but used `continue` when `claim_pending_command` failed. That allowed a later command to be claimed and executed in the same poll if the other writer released the lock between claims. For two queued `policy:<id>:Always` and `policy:<id>:Never` changes, the newer Never could be applied first and the older Always could be applied on the next poll, reversing the user's final choice. This is a scheduling interleaving supported by the production code, **not** a reported incident or a measured frequency.

## Correction and production-path reproduction

A single dispatcher now claims commands strictly in queue order and **stops the current poll** when the oldest pending action cannot be claimed. It retries the untouched queue at the next normal monitor poll. The existing SQLite transaction still prevents duplicate claims; already-running commands remain non-replayable, and explicit shutdown remains authoritative. The normal asynchronous scan handling, action completion, heartbeat and event auditing are unchanged.

`src/engine/command_queue_order_tests.rs` uses the real Store, actual SQLite WAL writer contention, real `.url` source, the production policy-command handler, and the same dispatcher as `monitor`. Its deterministic boundary releases the competing writer immediately after the first failed claim: **before**, the loop would attempt the second claim; **after**, only one claim attempt occurs, both commands remain pending, and a later poll executes Always then Never exactly once. It checks final policy, source bytes, pending rows and audit event count. A second scenario sets the real shutdown flag after the first claimed action and verifies the next action stays pending rather than starting after shutdown. The injected release ordering is synthetic; database and action code are production implementations.

## Acceptance and remaining work

Require Windows Server 2022/2025 locked Rust tests, formatting, warnings-denied Clippy, build, native integration and publication guards. The test doesn't simulate real user timing, lengthy storage stalls, Windows 11 Explorer, physical multi-monitor placement or interactive GUI scaling. Do not infer actual human time savings. Draft organizer PRs #98/#101 are independent and remain outside this change.
