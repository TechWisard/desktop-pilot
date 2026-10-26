# Capacity reduction during source outages and transactional usage state — 2026-10-26

## Motivation: evidence versus interpretation

Primary HCI research (not product-specific):
- Leah Findlater and Joanna McGrenere, CHI 2004, *A Comparison of Static, Adaptive, and Adaptable Menus*: a 27-person comparison found that controlled, adaptable arrangements were often preferred to automatically shifting ones. https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/
- Gajos et al., CHI 2008, *Predictability and Accuracy in Adaptive User Interfaces*: both predictability and accuracy improved satisfaction; accuracy improved actual use and performance in their study. https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml

Anecdotal user reports (not measurements of this software):
- A Windows 11 user reports that icons change positions despite manual placement after restarts. https://www.reddit.com/r/Windows11/comments/1fzfy3y/
- A user reports missing shortcuts around external-drive disconnection, without establishing the cause. https://www.reddit.com/r/techsupport/comments/ir0ok8

Design hypothesis: a deliberate reduction in desktop capacity should free an unusable automatic entry before a usable one even if past usage heavily favors the unavailable app. Explicit `Always` rules still override this default. This is a policy choice based on predictability and utility; no human efficiency gain has been demonstrated.

## Confirmed reproducible defects, from actual implementation

### Capacity reduction

Before: `model::select` retains a temporarily unavailable, already-owned `Auto` app during its 72-hour grace period. When the user reduces `Config.slots` from 2 to 1, both the offline app and a usable owned app are removable. The old pruning step chooses the *lowest score*, regardless of current validity. Example: offline score 200, usable score 20. It evicts the usable app. The normal replacement loop cannot re-add the usable app because its 20 points do not exceed the offline app's 200 points by the configured advantage.

After: while resolving excess capacity, non-manually-protected unavailable entries are discarded before usable ones. Original ordering and score comparison remain unchanged among apps with the same availability. `Always`, manual positions, default 72-hour grace, Never and other filters remain unchanged.

Real test paths:
- `model::tests::shrinking_capacity_prefers_an_available_copy_over_high_ranked_unavailable_one`: full production rank and selection, normal capacity, reduced capacity and explicit manual exception.
- `engine::tests::reducing_slots_removes_an_unavailable_copy_not_a_working_one`: real SQLite Store, discovery, preview, apply, real file copies, disappeared source and capacity change. Compare selected identities and file-preservation assertions.

Synthetic result: available owned copies after reduction changed from 0/1 to 1/1 in this constructed case. This is **not** a claim of real-world time savings or generalized ranking quality.

### Usage checkpoint failure

Before: `record_usage` writes a day's usage and the session checkpoint within one SQLite transaction but mutates the in-memory `session_times` cache before `store.set("app_session_times")` succeeds. A failure can roll back the database while leaving the mutable cache advanced. Such a failure currently makes the background monitor return an error, so the immediate user-visible impact is limited; the inconsistency still violates retry safety.

After: stage a copy of the cache, write both SQLite changes atomically, and update the cache only after successful commit. No changed database schema, data retention, tracking, or telemetry.

Test: `engine::tests::failed_usage_commit_does_not_advance_in_memory_session_history` installs a SQLite trigger that aborts the checkpoint update. It asserts rolled-back usage *and* unchanged cache, removes the trigger, retries through production `record_usage`, then verifies one saved session.

## Before/after verification

Red test-only run: https://github.com/TechWisard/desktop-pilot/actions/runs/40325563091 (Windows 2025: 164 existing tests passed, three new tests failed; formatting and release guards passed). The earlier test-only run https://github.com/TechWisard/desktop-pilot/actions/runs/40325313363 was stopped by formatting errors; it does not establish behavioral failure.

After the fix, check branch and merged-main CI for `cargo fmt --check`, `cargo test --locked`, Clippy warnings denied, build, release guards and real Windows native fixtures on both runner versions. Screenshots and physical UI input are not newly validated by these tests.

## Next independent scenarios

1. Weekend / five-day source absence: measure when the 72-hour grace expires, whether the decision is explained, and whether returning sources preserve manually placed copies.
2. Repeated project switching with a rarely but critically used `Always` app; assert zero unsolicited movement of the manual anchor.
3. Interleaved catalog scans, manual policy edits and profile changes; verify conflict reporting, no dropped intent and stable copy ownership.
4. Grace-period expiry while changing capacity, multiple offline and online apps, and a physically disconnected drive (beyond the current synthetic file-removal fixture).
5. Storage write failure during an actual worker loop followed by application restart; verify operational recovery and whether the UI exposes an actionable error rather than silently stopping.
6. Physical Windows 11 Explorer, multiple monitors/DPI and interactive usability validation. These cannot be inferred from Windows Server CI.
