# Keep exact inline movie trailers beside their source — 2026-11-10

## Reproducible risk and primary evidence

An explicitly enabled ordinary-file cleanup could separate `Feature.mkv` from `Feature-trailer.mp4`. If one file had already been excluded by a known dependency or a manual choice, the other could still be moved into `Media`. This is a source-confirmed potential association break; it is not a verified user complaint.

[Emby's trailer naming documentation](https://www.emby.media/support/articles/Trailers.html) specifies `MovieName-trailer.mp4` alongside `MovieName.mkv`. [Plex's local extras documentation](https://support.plex.tv/articles/local-files-for-trailers-and-extras/) independently specifies the `-trailer` suffix for inline extras, while also recognizing different descriptive names within movie folders. The latter cannot be safely inferred from a loose Desktop file list, so this correction intentionally only matches exact base stems.

## Behavior

For two top-level files whose case-insensitive stems are `Movie` and `Movie-trailer`, keep **both** in place when opt-in file organization is requested. A standalone `Unrelated-trailer.mp4`, a `Movie-trailer-cut.mp4`, and independent videos continue to organize normally. Do not open video content, recurse through directories, add new UI or preferences, change user policies, or alter the saved catalog. The default shortcut-only organizer is unchanged.

## Production regression and expected measurements

The regression executes actual Windows filesystem, SQLite Store, production Analyze, Apply and Undo: one movie/trailer pair, two independent videos, a real destination folder, byte preservation and no pending recovery after Undo. The stale-preview test adds `Feature-trailer.mp4` after Analyze and requires Apply to reject the entire proposed move before creating a folder. A unit test covers Unicode, case variants, an unrelated trailer and similarly named clips.

Before correction, the movie and trailer were both eligible for a new `Media` group if enough files were selected. After correction, their precise association keeps both, while two independent videos still move (2 moves, 1 new folder, net 1 desktop icon freed). This is a synthetic correctness expectation, not measured user productivity. Verify actual exact-commit Windows tests before merging.

## Release boundary

Keep this isolated in the unreleased 0.3.0 integration branch. Do not change stable main or publish the organizer until physical Windows 11 multi-monitor/DPI, active OneDrive redirection, keyboard accessibility and power-interruption checks pass. The existing free Windows CI, native Explorer, Cloud Files and UI checks remain mandatory. No additional artifact or cache steps.
