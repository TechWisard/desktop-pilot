# Preserve exact-name video metadata and artwork — 2026-11-10

## Practical risk and external evidence

Opt-in ordinary-file organization uses a bounded Desktop directory snapshot. Previously `Feature.mkv` and `Independent A.mp4` could both move into `Media` while `Feature.nfo`, `Feature-poster.jpg`, and `Feature-fanart.png` stayed on Desktop or moved to `Images`. The existing sidecar recognizer omitted these precise conventions. This is confirmed from production source and the reproducible fixture design, **not** a reported user data-loss incident.

Primary naming documentation: [Kodi movie NFO files](https://kodi.wiki/view/NFO_files/Movies) recommends `<VideoFileName>.nfo`; [Jellyfin NFO metadata](https://jellyfin.org/docs/general/server/metadata/nfo/) recognizes same-name movie and episode NFO; [Emby movie naming](https://emby.media/support/articles/Movie-Naming.html) documents `{name}-poster`, `{name}-cover`, `{name}-thumb`, and `{name}-fanart` image companions. Gajos et al., [CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), supports prioritizing accurate and predictable adaptive behavior, but does not quantify Desktop Pilot productivity.

## Correction and bounded scope

Use one names-only, case-insensitive video-stem index for files already in the current Desktop snapshot. Keep exactly matching `Video.nfo` and four documented `Video-<artwork>` image families beside their source video. This prevents moving the video *or its image companion* into different folders. Do not read file contents, recurse, follow new links, add settings/telemetry or block unrelated videos and photos. A `Photo.nfo` without a corresponding video does not keep `Photo.jpg`. Continue ignoring unsupported NFO as an independent candidate. Existing manual choices and default shortcut-only mode remain unchanged.

## Real-program regression and measurements

New production-path Windows/SQLite tests in `src/organize/tests_cases.rs`: real analyzer/Apply/Undo, a protected video with NFO and two artworks plus two independent videos and a manual Always shortcut with usage history; two separate late-arrival scenarios; exact matching and unrelated photo/metadata cases. The new unit test uses 4,000 actual indexed names without content reads.

**Before correction, source-derived expectation:** the protected video/artwork could be proposed for separate moves and late associated files would not invalidate an old plan. **After correction, expected passing scenario:** two independent video moves, one created `Media` folder, one net desktop slot freed; the metadata-associated video and art remain byte-identical in place. Late NFO/artwork must reject Apply before creating any folder. Independent files continue organizing; Undo restores original bytes, while manual rules/history are unaffected. Verify actual Windows CI outputs before marking this proven. These are synthetic correctness and slot counts, **not** measured human time savings.

## Release and storage boundaries

This is an isolated organizer change targeting the unreleased 0.3.0 integration branch. The stable main/release and physical Windows 11 multi-monitor/DPI, live OneDrive account, interactive accessibility and physical power-interruption acceptance gates remain untouched. Use the existing free public CI; preserve required license and workflow controls. No extra artifact/cache generation is introduced.
