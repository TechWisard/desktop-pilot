# Paused collection must suspend unattended adaptation — 2026-10-31

## User-facing requirement and prior evidence

Baseline public main: `e31f6e856ac81f5a14725340542980822e749ab7`, last published binary 0.2.69, no open PR. Main has working public Windows 2022/2025 checks; no alternative runner or repository is needed.

The documented **Adaptive** mode both observes activity and changes shortcuts during quiet periods; **Observe** only proposes changes; **Manual** requires explicit actions. A concurrent three-way Settings save can also legitimately preserve `automatic=true, collect=false`: the user pauses collection while retaining the adaptive preference for later. This is already tested in `src/ui/mode_merge_tests.rs` and explained in [operating-mode conflicts](2026-10-30-operating-mode-conflicts.md).

The prior [adaptive menu literature](2026-10-26-adaptive-shortcuts.md) establishes the importance of direct user control and spatial predictability; it does **not** measure Desktop Pilot usage or prove a human time saving. No additional user telemetry or research collection was performed.

## Verified defect on the real production path

The worker's monitor loop deliberately disables input hooks when `Config.collect=false`, but it still calls `engine::automatic_review` during periodic maintenance. That function previously required only `Config.automatic`, not `collect`. When a paused Adaptive profile retained enough older history, was past its ordinary review date, and the desktop was quiet, it could **move quick shortcuts automatically while tracking was paused**. The Settings health label could simultaneously say it was actively adapting instead of paused.

Failure-first regression `model::cold_start_tests::paused_collection_must_not_apply_stale_adaptive_recommendations` runs with the real isolated SQLite Store, two native-parsed inert Internet Shortcuts, a one-slot desktop, actual `engine::apply` and `automatic_review`, historical scores and an overdue review. It first establishes a real managed old quick shortcut and a higher-ranked new candidate, then pauses collection. The fixture requires no changes during pause, unchanged source bytes, no new/deleted desktop copy and unchanged review timestamp.

**Before the fix:** the formatted test reached `cargo test --locked` on both Windows Server 2022 and 2025 in [Actions run 41151233208](https://github.com/TechWisard/desktop-pilot/actions/runs/41151233208). **334 tests passed and this one failed** on each runner at the specific assertion that paused tracking must not make unattended changes. The earlier run that stopped at rustfmt is not behavioral evidence.

## Correction, positive and negative controls

`engine::automatic_review` now requires **both** `automatic` and `collect` to be enabled before unattended placement. It still requires learning readiness, a due review and a quiet desktop. Manual `Apply`, stored recommendations and manual rules remain available: pausing does not destroy user history or preferences. Reactivating collection allows the next ordinary due review to proceed. The real fixture then checks one permitted replacement, actual file bytes, Undo restoring the original shortcut and suspension of automation after Undo.

`ui::health` now labels the special paused-Adaptive configuration as **Observation paused**, not as active automatic adaptation. A companion regression checks the paused, resumed, Observe, Manual and stopped-worker status transitions.

An independent acceptance in [mixed-project clock recovery](2026-10-31-mixed-project-clock-recovery.md) checks preserved session counters and timestamps for two projects after a large clock correction. Its first hypothesis was rejected against the existing verified recovery behavior; it is not a production algorithm change.

Two older clock-correction tests had treated `automatic=true` alone as full adaptive permission, relying on the previous unguarded behavior. Their fixtures now explicitly enable collection for normal automatic review; the manual-shortcut test also asserts that a paused intermediate state **does not** review. This preserves their original clock-correction guarantees while extending the opt-in gate, rather than disabling or deleting a test.

No change to UI density, new settings, permissions, dependencies, log collection, telemetry, originals or rollback format. The fix does not claim to solve an in-flight race where the user pauses **during** an already-running apply; that requires separate synchronization analysis and a deterministic test.

## Verification and remaining work

Keep the existing Windows 2022/2025 locked Rust suite, format, warnings-denied Clippy, build, native integration and publication checks unchanged. Verify the exact branch result and merged main result before publishing binaries, and verify release tag, executable version, assets and checksums.

Remaining: physical Windows 11 UI at 200% DPI, real disconnected network roots, prolonged changes in work habits, and measured first-use benefit. CI regression checks are not evidence of performance or time savings for real users.
