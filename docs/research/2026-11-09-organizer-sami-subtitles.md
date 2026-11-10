# Preserve SAMI external subtitles with their videos — 2026-11-09

## Observed code gap and real use

The opt-in ordinary-file organizer recognizes MP4/MKV/AVI/MOV/WebM as movable Media. The existing production sibling guard protects SRT, ASS, SSA, VTT, SUB and IDX (including language/forced suffixes) but omitted SAMI `.smi`. A source-matched `Episode.en.smi` could therefore be left on Desktop while `Episode.mp4` moved to Media. This is a reproducible *risk* in the code, not a reported data-loss incident.

Primary references: [Plex — Adding Local Subtitles](https://support.plex.tv/articles/200471133-adding-local-subtitles-to-your-media/) explicitly lists SMI plus `Movie_Name.eng.smi` and language/forced patterns; [Kodi Subtitles](https://kodi.wiki/view/Subtitles) also supports SAMI external subtitles. These sources establish the file association, not user-specific frequencies.

## Minimal correction

Include `smi` in the already-existing nonrecursive, bounded, name-based dependency guard. The guard recognizes exact video stem with the documented `.language` and `.forced` suffixes; it never opens subtitle content, never follows arbitrary paths, and remains active in both Analyze and final Apply revalidation. No new options, telemetry, automatic file movement or policy changes.

## Actual program regressions

- Sidecar test: exact named `Episode.mkv` + `Episode.eng.smi` protects the video while a nonmatching subtitle cannot capture a shortcut. Existing SRT, photo editor sidecars, archives and lyrics remain protected.
- Real synthetic Windows Desktop + SQLite: `Эпизод%USERNAME%.mkv` and `Эпизод%USERNAME%.eng.forced.smi` remain byte-identical while two independent videos and two documents organize into category folders, then Undo restores them. Saved Always policy and usage history remain unchanged; the journal closes.
- A matching SAMI subtitle arriving after Preview blocks the stale two-video Apply before any rename or new category folder; a fresh preview explains that the video stays.

The final exact-head CI must pass on Windows 2022/2025/Windows 11 ARM, including fmt, locked Rust tests, strict Clippy, native Explorer, Cloud Files and egui screenshots. This does not constitute a physical hardware, user time-saving, multi-monitor or real OneDrive test, and does not authorize release 0.3.0.
