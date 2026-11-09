# Keep camera stills, RAW originals and Live Photo movies together — 2026-11-09

## Evidence and actual source behavior

- **Primary source, Adobe Lightroom Classic:** its import preference specifically treats adjacent RAW+JPEG as either separate photos or a JPEG sidecar associated with the RAW: https://helpx.adobe.com/lightroom-classic/desktop/import-photos/file-import-formats-settings.html . This confirms a real file-association workflow; Desktop Pilot does not know how Lightroom is configured.
- **Primary source, Apple Photos:** exporting an unmodified Live Photo produces separate still-image and video files: https://support.apple.com/en-gb/guide/photos/pht6e157c5f/12.0/mac/27 . Apple's developer documentation says the actual relationship is identified by content identifiers in photo/movie metadata, not filenames alone: https://developer.apple.com/documentation/avfoundation/avcapturephotosettings/livephotomoviemetadata .
- **Independent user experiences:** photographers discuss same-basename RAW+JPEG pairing and issues treating pairs as separate files at https://www.reddit.com/r/Lightroom/comments/lqq9i3/ ; an Apple Photos user describes an exported HEIC+MOV pair at https://www.reddit.com/r/ApplePhotos/comments/1i76sve/ . These are experiences with other software, **not reports of Desktop Pilot damage**.

**Reproduced from the production code, not inferred from those reports:** the unreleased opt-in ordinary-file organizer classifies JPG/JPEG/HEIC as Images, MOV as Media, and camera RAW formats such as CR3/NEF as unknown. Its existing sidecar guard handles XMP, subtitles, camera thumbnails and multipart archives but did not match a RAW original beside a JPEG, or an image and a MOV of the same basename. Consequently a JPEG could move away from its RAW original; an exported Live Photo's still and motion could be sent to different folders.

## Reproducible Windows test-first results

At the parent implementation, new native Windows/SQLite tests reproduced both gaps in [CI 42526474895](https://github.com/TechWisard/desktop-pilot/actions/runs/42526474895) (Windows 2022 log): **548 existing tests passed, two new tests failed, three special integration tests were ignored by the ordinary unit invocation**. The initial test-only commit also failed rustfmt; formatting was corrected subsequently, not disabled.

- 'optional_camera_pairs_stay_together_while_independent_media_moves_and_undoes': with real regular files on a disposable Desktop, the unchanged analyzer proposed DSC_1001.JPG independently of DSC_1001.CR3; it was missing from the protected list. The checked scenario contains a RAW+JPEG, HEIC+MOV, JPEG+MOV and four independent files. Before protection, nine eligible objects could move across Images/Media; the intended safe plan contains only four independent objects, with two new folders.
- 'camera_companion_arriving_after_preview_rejects_all_moves_without_a_journal': a matching companion created after preview did not invalidate Apply. The old engine returned Outcome { moved: 4, folders_created: 2, ... } instead of rejecting the stale plan. This is evidence of a real logic hazard on disposable test files, not an observed user data-loss incident.

## Change and rationale

The camera-pair matcher in src/organize/plan/camera_pairs.rs indexes the same bounded, non-recursive Desktop filename snapshot already used by Analyze and re-evaluated by Apply. Case-insensitive **exact stem** and a conservative extension allowlist identify plausible RAW+JPEG pairs and Live Photo still (HEIC/HEIF/JPEG)+MOV pairs. Only the eligible companion files are excluded from the move plan; RAW already remains excluded by classification. All paired resources stay where they are. Independent JPG and MP4/MOV files without a matching partner remain movable. Existing multi-file journals, manual choices and Undo are unchanged.

The matcher does **not** read file contents, access image metadata, identify photo subjects, open archives, use external services or silently combine files. The heuristic deliberately prefers an occasional false positive (a similarly named photo and video staying put) to splitting an actual pair. Identically named resources are *not* proven to be a Live Photo without checking Apple content identifiers. Differently named pairs and uncommon export formats remain outside this guarantee.

Unit tests cover mixed-case extensions, literal percent signs, unrelated names and a 6000-item snapshot; Windows production-path tests cover actual Analyze -> Apply -> Undo and an arriving-after-preview companion. Normal ordinary-file movement is still opt-in. Pending final Windows checks must verify before/after, independent file bytes, absence of partial journals on rejection and unchanged UI/search behavior. Synthetic results are not proof of reduced real-world mistakes or time saved.

## Release boundary

This change is scoped to the unreleased 0.3.0 organizer integration, not stable v0.2.128. Full checks on the exact feature and PR merge head remain mandatory, together with independent physical Windows 11 multi-monitor/DPI, active OneDrive, interactive keyboard/accessibility and power-loss acceptance before any organizer release. The concurrent document-lock PR #250 owns a different module; preserve it without force-pushing or discarding its history.
