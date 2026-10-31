# Organization: linked media, Undo ordering and visible feedback

Date: 2026-10-31. Continuation of PR #98, not a replacement implementation.

## Starting point and scope

The existing feature branch already contains read-only analysis, a paginated preview with exclusions, conservative opt-in file grouping, durable move journals, catalog path updates, worker serialization and recovery. This continuation branched from `dd6b18273c1ccb2112fc5f8f2820b36e7964df69` to avoid overwriting parallel work on `feature/desktop-organization-ui-20261031`.

The question tested here is not merely whether files can move: can a user organize the desktop, continue using quick shortcuts and ordinary media, then recover without breaking related files or locking one Undo mechanism behind another?

## Primary evidence and engineering decisions

- The [darktable manual](https://docs.darktable.org/usermanual/3.6/en/overview/sidecar-files/sidecar-import/) describes both `name.extension.xmp`, numbered edit versions and `name.xmp`. Its [sidecar documentation](https://docs.darktable.org/usermanual/development/en/overview/sidecar-files/sidecar/) explains that edit information lives alongside the original. Moving the image alone can separate that information; this is a dependency risk, not a claim that the image bytes are corrupted.
- VideoLAN's [VLSub source and built-in usage instructions](https://github.com/videolan/vlc/blob/master/share/lua/extensions/VLSub.lua) describe saving subtitles beside the video under the same basename so the player loads them automatically.
- The previous adaptive-interface research remains applicable: preserve user control and make consequential changes understandable. It is not necessary to repeat the complete literature search for this focused patch.

A matching sidecar filename is evidence of a possible dependency, not proof that the files belong together. The safe decision is to leave the recognized set in place, explain why, and still organize unrelated groups. It is not safe to execute applications, parse arbitrary scripts or silently assume that every file with a known extension is independent.

## Confirmed failures and fixes

### Media files and sidecars

The optional file pass previously treated a JPEG next to `Photo.jpg.xmp` as independent. The production `analyze`/`apply` tests in run [41211825998](https://github.com/TechWisard/desktop-pilot/actions/runs/41211825998) failed for the existing-pair case and for a subtitle appearing after preview.

The new bounded dependency index uses the same at-most-6000-entry directory snapshot already required by analysis and Apply. It recognizes common edit sidecars, subtitles, language/variant suffixes and audio cue files, with case-insensitive and Unicode filenames. No per-file directory scan, sidecar content parsing, new dependency or persistent cache was added. The index is rebuilt at Apply, so a sidecar saved after preview invalidates the stale plan before the first folder or move.

The integration fixtures check five real file combinations: full-name XMP, basename XMP, plain subtitles, language-suffixed subtitles and a cue sheet. Unrelated website shortcuts still move and undo, while the linked media and sidecar bytes remain untouched. Two additional index tests cover numbered edit naming and a full 6000-name synthetic snapshot.

These protections do not discover every external dependency. Sidecars created after the final preflight, links stored in another application's private database, and nonstandard naming remain limitations. Ordinary files stay opt-in; unknown types and existing project directories remain excluded.

### Quick placement followed by organization Undo

The first compound fixture assumed a new quick copy would have a different filename from the moved original. Runtime evidence disproved that assumption: placement legitimately reused `Desktop/Alpha.url`, vacated by organization. The fixture was corrected to model the actual production behavior, not to change the naming policy.

Run [41212649799](https://github.com/TechWisard/desktop-pilot/actions/runs/41212649799), commit `2d4d7eeab2bbf8739a7e602ac5b281538aec0be2`, then reproduced the actual failure: organization Undo found the occupied original name after entering its durable recovery phase. That phase also blocked quick-placement Undo, which the user needed to free the name. Windows Server 2025 executed 384 tests: 383 passed and this one failed at the explicit non-pending-state assertion. Both sidecar integration tests and the strengthened modal tests passed in that run. Earlier formatting errors are not behavioral evidence.

The fix checks current managed file identity before beginning a fresh organization Undo. For a verified quick-copy collision it changes no files and leaves the organization journal committed, with no recovery barrier. The message in the saved language directs the user to undo quick placement on Home first, then retry organization Undo. It never deletes or replaces the quick copy. Already interrupted recovery retains its existing safeguards; unknown/user-owned collisions still use conservative recovery rather than being mistaken for managed files.

The compound regression exercises both request orders. It compares original file IDs, quick-copy IDs, usage rows, manual rules, catalog paths and owned state. It verifies that rejected organization Undo has not partially restored another file, then performs the actual quick-placement rollback and successful organization Undo. This is functional evidence, not a measured productivity benefit.

### Modal test validity

A newly created egui Area has an invisible sizing pass. Initial paint assertions were incorrectly inspecting that pass. The test now advances a bounded three sizing frames with increasing input time, then requires visible feedback and Close controls in every subsequent tested frame. It also sends real pointer press/release events to Close, checks pending-operation text inside the modal, and verifies geometry in Russian/English at 100% and 200%. It does not accept empty output as success and does not replace screenshot or physical-display review.

## Reproduction and acceptance status

Use the repository-pinned Windows toolchain and unchanged checks:

```powershell
cargo fmt --check
cargo test --locked organize::acceptance_tests
cargo test --locked ui::organization::tests
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
./scripts/test-native.ps1
```

Post-fix run results must be recorded in the PR after completion. No Windows execution was performed in the local Linux container; the measured runs above are public GitHub Actions jobs. No billing, repository visibility, security setting or required acceptance gate was changed.

The overall feature remains unreleased until the existing PR #98 acceptance checklist is satisfied: actual GUI review, live desktop position/Undo verification, running-worker pause/resume and conservative handling of synchronized desktops. Neither synthetic layout checks nor a green unit-test total proves those scenarios. The existing release remains unchanged.
