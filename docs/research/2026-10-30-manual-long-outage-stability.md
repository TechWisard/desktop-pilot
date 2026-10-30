# Manual-mode retention across extended source outages — 2026-10-30

## Starting point and provenance

Baseline: public `main` / release 0.2.66 at `c41671a16133601def1447999d1be59d91e0c5b4`, with successful [main Windows CI](https://github.com/TechWisard/desktop-pilot/actions/runs/41062120372) and no open PRs when work began. The prior [taskbar-pin stability regression](2026-10-30-manual-taskbar-pin-stability.md) protects a different Manual-mode case. The longer-running [source outage and cold-start task](https://github.com/TechWisard/desktop-pilot/issues/22) remains open.

Findlater and McGrenere, CHI 2004 (https://doi.org/10.1145/985692.985704), motivate predictable layouts and explicit control. This study **does not measure Desktop Pilot or provide evidence of its user time savings**. The Desktop Pilot defect below is confirmed by source and actual production-path Windows regressions; a ten-day outage is a controlled fixture, not a reported incident.

## Source-confirmed defect and failure-first reproduction

The selector `model::select_at` retained existing Auto copies from an unchanged unavailable source for at most 72 hours, even when the user had explicitly switched to `Manual` (both `collect=false` and `automatic=false`). After a long outage, pressing **Apply** would propose removing that existing useful quick copy despite no user's `Never`, movement, or capacity change. The catalog warning similarly described a 72-hour expiry independent of mode.

Two tests were added on top of the unmodified 0.2.66 implementation:

- `manual_only_preserves_owned_auto_copy_after_a_long_source_outage` invokes the real selector and preview with a ten-day outage; also covers Observe, explicit Never, changed identity, and reduced capacity.
- `saved_sqlite_preview_keeps_unmodified_copy_during_ten_day_outage_in_manual_mode` uses genuine isolated Windows `.url` file bytes, production `engine::refresh_apps`, SQLite `Store`, an existing owned desktop copy, and `engine::preview`. After correction it also checks actual `engine::apply`, warning wording, source/quick-copy integrity, explicit Never, and restoration.

The first test push failed `cargo fmt --check` on both runners; that was **not** behavioral evidence. After formatting, [failure-first CI 41064605803](https://github.com/TechWisard/desktop-pilot/actions/runs/41064605803) executed 327 Rust tests on **each** Windows 2022/2025 runner: **325 passed, both new tests failed** with the unexpected missing Manual selection. This reproducibly proves the source-level defect. Those red checks were performed before changing implementation.

## Narrow change and invariant

Treat an **already owned** Auto shortcut in Manual-only mode as retained through an unchanged source/target outage. Keep `Never` authoritative and continue rejecting changed launch identities, new copies from unavailable sources, and explicit slot reductions. Observe/Adaptive retain the 72-hour grace. The existing Manual-mode taskbar-pin behavior is unchanged; no new settings, storage migration, service, telemetric collection, or dependency.

Catalog warnings for a Manual-owned copy now say the existing copy is kept by user choice, rather than claiming it will expire after 72 hours. This matters when a source is temporarily disconnected for days during a holiday or a project switch.

The regression's pre-correction **unwanted removals proposed: 1**; target **0** in Manual with no explicit user change. Preserve original/owned bytes and number of unrelated shortcut placements. These are deterministic correctness/operation counts, **not** measured human time or energy savings.

## Required verification and limitations

Run unchanged Windows 2022/2025 formatting, locked Rust tests, strict Clippy, locked build, README/9 publication guards, and 29 native fixtures. Inspect CI on the final commit, not intermediate greens. If the change warrants a release, independently verify release commit, tag, built EXE, archive contents and SHA-256.

Still not proven: physical Windows 11 at 100%/200% DPI, real network/cloud-source stalls, actual user's desktop geometry, first-result latency with large catalogs, hook CPU cost, and multiday changes of application habits. Issue #22 tracks the larger acceptance; issue #56 tracks hook measurements.
