# Repeated organizer projection work in the UI — 2026-11-06

## Scope and existing evidence

Continuation from draft organizer PR #98 at `b8be9f15d3f79d9b67434b5b3c02b323b6642304`; no change to released `main`. The previous investigation `2026-11-05-organizer-projected-public-hardlinks.md` establishes why path identity is important to this estimate. The root problem here is different: repeated work when the preview itself has not changed.

The official [egui immediate-mode guide](https://docs.rs/egui/latest/egui/) documents that UI-building code runs again for every frame (for example, dozens of frames per second). [Microsoft's directory-handle documentation](https://learn.microsoft.com/en-us/windows/win32/fileio/obtaining-a-handle-to-a-directory) and [GetFileInformationByHandle contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfileinformationbyhandle) describe the file and directory handle operations used for reliable identity checks. These are primary descriptions of mechanisms, not measurements of Desktop Pilot latency.

## Source-confirmed repeated-work defect

`Settings::organization_dialog` called `Plan::projected_zone` unconditionally on every UI frame merely to decide whether to paint a warning. The production calculation searches grid positions and, for name aliases, opens directory and file handles to verify identity. Neither the saved plan, the selected checkboxes, nor the configuration necessarily changes from frame to frame. The repeated calculation has no user-facing benefit and may delay UI response on crowded desktops or slower storage. No real-user freeze duration or energy cost has been measured.

## Fix and reproducible checks

Cache **only the read-only advisory projected-zone availability** for a specific plan, full config and complete selection flags. Recompute when the user changes any checkbox or configuration. Invalidate the cache when analysis begins, a new plan arrives, options invalidate the preview, an operation finishes or an exclusion reset occurs. A fresh analysis is mandatory to incorporate external desktop changes.

The regression test `unchanged_organizer_frames_reuse_production_projection_but_edits_invalidate_it` exercises the actual generated organizer plan, actual `Plan::projected_zone` and Windows filesystem setup. It counts production projection calls across 40 unchanged egui-style frames: **40 before, 1 after** (calls, not milliseconds). It then verifies that selection and configuration changes each require a new production computation and that re-analysis clears the cache. Files, SQLite state and recovery journal are not modified by a projection.

The projection is still a non-authoritative estimate. `apply_stopped` independently rechecks all originals, destination collisions, current desktop geometry, rules and catalog identity under the existing exclusive locks before changing any files. The cache never grants permission to move files.

## Acceptance and remaining limits

Run unchanged strict Windows Server 2022/2025 Rust, format, Clippy, build, native and publication guards for the exact branch head. Green synthetic UI tests do **not** establish smooth physical Windows 11 100–200% DPI interaction, disconnected OneDrive behavior, multi-monitor layout safety or task-level human time savings. Leave PR #98 in draft and do not release the organizer until those acceptance gates are satisfied.
