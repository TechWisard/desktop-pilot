# Protect literal-percent photo names and XMP sidecars — 2026-11-07

## User-visible scenario and evidence

On an opted-in file organization preview, a photograph and an adjacent XMP editing sidecar are **one working set**, even though the organizer never moves the sidecar itself. Adobe's Camera Raw guidance requires the sidecar to remain alongside its source asset and explains why metadata/edits may be lost when the pair is separated: https://helpx.adobe.com/ca/camera-raw/desktop/get-started/overview-and-setup/camera-raw-settings.html . The darktable manual likewise describes separate XMP and DTDATA editing-history files beside images: https://darktable-org.github.io/dtdocs/en/overview/sidecar-files/sidecar/ .

Windows expands `%NAME%` in command-processor templates, not when identifying a *filename already returned by directory enumeration*: https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd . Source inspection of `src/organize/plan/sidecars.rs` confirmed that `protected()` used `model::norm` (environment-expanding) for the enumerated photo's filesystem path, whereas `Dependencies::check` compared the enumerated path with `norm_literal_path`. For a real folder named `%USERNAME%` or a photo named `Photo%USERNAME%.jpg`, the protected lookup key differed from its candidate, allowing a photo with `Photo%USERNAME%.jpg.xmp` to enter the move plan while the sidecar remained behind.

This is a source-confirmed issue with a deliberately constructed filename, **not** an observed customer incident or evidence of saved minutes. The prior adaptive-interface studies and manual-control rationale remain in [adaptive-shortcuts](2026-10-26-adaptive-shortcuts.md); no need to repeat the general search.

## Correction and production-path regression

Use literal path normalization for the sidecar protection index. Continue retaining the existing case/slash equivalence without expanding embedded percent tokens. This does not change launch target expansion, manual rules, source contents, local storage or activity collection.

The Windows regression `literal_percent_names_keep_photo_and_its_xmp_sidecar_together` uses the real SQLite store, an actual disposable `%USERNAME%` folder, four real files, production `analyze`, `apply` and `undo`. It requires the XMP-associated photo to be skipped explicitly while two unrelated photos are moved and restored. It checks original bytes, absence of a stuck recovery barrier and the category folder's removal on Undo. Existing sidecar naming, bounded 6000-object, optional files, stale-preview and interruption regressions must also pass.

Synthetic before/after: a single paired photo was erroneously eligible before the literal-key correction; afterwards it is not eligible, while both independent photos remain eligible. This is safety evidence for this fixture, not real-world productivity measurement.

## Release boundaries

Keep this change on an isolated branch targeting the pending draft integration PR #211. Require the unchanged public Windows formatting, Rust tests, strict Clippy, build, native checks and publication guards. Do not ship the broader organizer until Windows 11 live visual review, OneDrive and physical multi-monitor/interruption acceptance are complete. No telemetry or user-file migration.
