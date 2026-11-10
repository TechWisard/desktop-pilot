# Mixed document and camera companions: integration acceptance — 2026-11-09

## Motivation and evidence
The source-verified optional-files hazards and their external primary sources are recorded in [the Office owner-file research](2026-11-09-organizer-editable-document-owner-locks.md) and [the camera pair research](2026-11-09-organizer-camera-pair-integrity.md). Microsoft Word can create a `~$` owner file while a document is open; LibreOffice can leave a `.~lock.<name>#` marker. Apple Photos exports Live Photos as still and movie components, and Lightroom treats some RAW+JPEG pairs as linked files. These are documented conventions, **not** reports of Desktop Pilot losing user data.

PR #250 and PR #251 each passed real production-path Windows/SQLite fixtures independently, including a late-file Apply rejection. This extra acceptance combines both sets in **one** bounded Desktop snapshot, with independent files and ordinary site shortcuts still eligible. Its purpose is to detect cross-feature interference or an overbroad guard, not to add UI settings or claim actual productivity measurements.

## Production-path test and expected result
`mixed_document_and_camera_dependencies_survive_apply_undo_and_late_arrival` in `src/organize/tests_cases.rs` uses the shipped analyzer, SQLite store, Windows filesystem, journal and Undo. It creates real synthetic Word owner, LibreOffice marker, CR3+JPG and HEIC+MOV pairs alongside two independent documents, two independent photos and two known site shortcuts.

Expected: default shortcut-only review proposes 2 moves and 1 folder; explicit ordinary-file review proposes **6 moves in 3 folders**, excluding every recognized companion. Apply moves precisely the independent six. Undo returns every original file and recorded site shortcut, preserving bytes, catalog identity, history and disabled background permission. Then a new JPEG becomes eligible; a matching NEF added **after preview** makes Apply reject the entire stale plan with zero new moves/folders/journal.

This is a synthetic correctness and composability check using production operations. It does not establish measured user time savings, detect every document-editing protocol or identify Live Photos by content metadata. Both filename matchers deliberately prefer conservative keeping to breaking known file relationships.

## Acceptance and release boundary
The exact new commit needs full public Windows 2022, 2025, Windows 11 ARM CI: locked Rust tests, strict fmt, Clippy, native/Explorer/Cloud Files checks and screenshot fixtures. Do not merge on pending or failed checks. The unchanged, physical Windows 11 multi-monitor/DPI, active OneDrive account, keyboard/accessibility and physical power-interruption gates still block v0.3.0. Stable main and published v0.2.128 are untouched.
