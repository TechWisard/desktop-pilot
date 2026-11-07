# Associated editor and camera files during optional Desktop grouping

## Confirmed source and user-visible failure

Desktop Pilot moves ordinary photographs and videos only when the user explicitly
enables file organization. The existing analyzer recognized XMP, captions and
cue sheets, but not Lightroom Classic 15's separate ACR editing sidecar or
GoPro thumbnail/preview files. A photograph such as `Edited.jpg` beside
`Edited.acr`, or a GoPro master `GX012345.mp4` beside
`GX012345.thm` and `GL012345.lrv`, would therefore have been offered for
relocation while its companion remained on the Desktop. This is a source-level
reproduction of a real dependency risk, not a measured user incident.

Primary sources:
- Adobe Lightroom Classic, *Save metadata to external sidecar files*,
  updated October 27, 2025: Lightroom 15 creates an additional `.acr`
  sidecar to retain heavy photo edits:
  https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/create-xmp-acr-files.html
- GoPro, *What Are .thm And .lrv Files?* (September 6, 2023):
  camera thumbnails and low-resolution previews are necessary for Quik's
  copy, playback and sharing operations:
  https://community.gopro.com/s/article/What-are-thm-and-lrv-files?language=en_US
- GoPro, *Camera File Naming Convention* (official): GH/GX prefixes for
  video clips and numeric chapter/clip suffixes:
  https://community.gopro.com/s/article/GoPro-Camera-File-Naming-Convention?language=en_US

## Behavior and scope

- Treat matching adjacent `.acr`, `.thm` and `.lrv` names as dependency
  evidence. Photo and video masters stay at their original location, rather
  than moving sidecars automatically or silently modifying names.
- For the GoPro `GL######.lrv` variant, protect an actually present
  `GH######.mp4` or `GX######.mp4` (also MOV). Numeric format and
  exact sibling filenames restrict accidental matches.
- Unrelated photographs/videos remain eligible for the explicitly approved
  file move. The default shortcut-only setting is unchanged.
- The conservative skip is intentional: a sidecar may be unrelated even
  when named alike. The user can resolve that association before trying
  again; the organizer will not speculate about file contents.

## Reproducible checks

`src/organize/plan/sidecars.rs` tests the production dependency detector
with case variants, exact, extension-bearing and GoPro GL/GX conventions,
unmatched sidecars and a 6000-object snapshot.

`optional_media_organization_keeps_editor_and_camera_companions_in_place`
uses the real Windows analyzer, SQLite Store, filesystem Apply and Undo. It
checks that default preview moves nothing, two associated masters are skipped,
four unrelated files can be grouped, companion bytes never move, and Undo
restores the independent originals. Test metrics describe synthetic fixture
behavior only; no human time-savings claim is made.

## Acceptance and remaining risk

Run unchanged Windows formatting, test, strict Clippy, build, native fixtures
and live Explorer/UI acceptance on the exact candidate before integration.
This protects only recognized name conventions, not all possible cross-file
dependencies; references embedded in custom media project formats still
require user judgment. The larger desktop-organization feature remains in
draft pending real-user desktop acceptance.
