# Source icon positions in Explorer's merged Desktop view — 2026-11-06

## Starting point and evidence

This continuation builds on draft organizer PR #98 at `086499166e7a427f332b4a75d1196eff90388615`, and the earlier notes `2026-11-05-layout-alias-undo.md` and `2026-11-05-organizer-projected-public-hardlinks.md`. Those studies already establish identity checks for projected occupancy and Undo. This change addresses a separate location where the source position was still selected by **file identity alone** during Analyze.

**Confirmed from source:** `src/organize/plan.rs::analyze` indexes Explorer's view paths and falls back when a shortcut's canonical path does not match a view path. The fallback previously used the first same-name entry whose file ID matched, without comparing its parent folder. This can conflate a personal Desktop source and a Public Desktop NTFS hard link. The resulting initial position is carried into the organizer journal and can later be used by layout restoration.

**Primary documentation:** [Microsoft, Hard links and junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions) states that multiple paths on one NTFS volume can refer to the same file. [Microsoft, BY_HANDLE_FILE_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information) describes the volume and file-index identity; this identity alone does not distinguish distinct directory entries. [Microsoft, Naming a file](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) documents alternate valid Windows path spellings. These platform contracts support the mechanism but **do not prove a Desktop Pilot customer experienced the defect**.

## Correction and regression

`plan::source_position` is now the same function called by the production `analyze` path and by the regression. It retains the inexpensive exact-path lookup. On an alias fallback it checks (1) leaf name, (2) actual file ID, and (3) identity of the containing personal Desktop folder. If multiple Explorer entries still match, no initial position is claimed. It never repositions, overwrites, or edits either source.

`analyzed_position_alias_must_belong_to_personal_desktop_not_public_hardlink` uses a real Windows filesystem and SQLite fixture with two .url shortcuts, a Public directory hard link to the original, and alternate `Desktop\\.\\` / `Desktop\\..\\Desktop\\` spellings of the personal directory. Test assertions cover:

- With only a Public hard link in the Explorer-like view, the selected personal icon has **no position**, not the unrelated Public position (the old fallback would choose it).
- Adding one same-parent valid alias yields that alias's position.
- Adding a second same-parent alias makes the position intentionally ambiguous; no arbitrary first match.
- An exact source path wins, and both files and the organizer journal remain unchanged.

These are **synthetic Explorer item entries passed to actual production resolution logic**. The filesystem identities are real; the test does not claim an actual merged Explorer view, real Windows 11 icon placement, latency improvement, or user time saved.

## Release and continuation gate

Run the unchanged Windows Server 2022/2025 formatting, Rust tests, strict Clippy, build, native fixtures and publication guards on the exact fix head and after integrating into draft #98. Keep the organizer unmerged and unpublished until physical Windows 11 Explorer/Undo at 100–200% scaling, multiple monitors, OneDrive/redirected desktop, worker pause/resume and interruption recovery are accepted. No paid resources or telemetry.
