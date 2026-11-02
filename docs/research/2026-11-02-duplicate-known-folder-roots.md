# Duplicate known-folder roots and the shared scan budget — 2026-11-02

## Observation, sources and hypothesis

Desktop Pilot scans user and common Desktop and Start Menu roots under a shared 6,000-shortcut limit. Previously, two identical resolved root paths each consumed that limit, while the duplicate shortcut paths were removed only after visiting every root. When the first root already contained enough shortcuts, a later root could be ignored even though its shortcuts were unique. This is confirmed from the production visitor and scan orchestration. No field incidence or measured time saving is claimed.

Microsoft's [known folders documentation](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders) describes redirection of per-user and common folder paths. [Microsoft support's OneDrive Desktop discussion](https://learn.microsoft.com/en-us/answers/questions/5595789/desktop-major-folder-path-issues) documents real mapping confusion. Neither proves that a personal and shared Desktop normally overlap; this is a reproducible edge case, not a prevalence claim. Previously captured [stable traversal research](2026-11-02-stable-bounded-discovery.md) and [scan pruning analysis](2026-11-02-irrelevant-scan-pruning.md) remain applicable.

## Reproduction with production traversal

The regression `duplicate_known_folder_roots_cannot_starve_shortcuts_in_later_roots` creates two actual Windows filesystem directories and two `.url` sources, then invokes the same root collector and visitor as the running scan, with a synthetic budget of two:

- **Before (code-path trace):** the first root's `Visible.url` is visited twice, reaching the cap before the Start Menu `Important.url`. Deduplicating at the end yields just one original.
- **After:** duplicate normalized roots are skipped *before* visitation. Both distinct original shortcuts are returned, bytes are untouched, and a list without duplicate roots produces the same result. The shared budget, existing sorted traversal and first-root error policy remain.

This guard recognizes normalized identical root strings without resolving aliases over the network or following reparse points. Physical Windows 11 redirection, slow/unavailable shares, Explorer, UI scaling, and real user task-time remain unverified. Continue the unchanged Windows Server 2022/2025 formatting, full Rust test, Clippy, build, native integration and publication checks. Keep organizer draft PRs #98 and #101 untouched; no telemetry or new user setting.
