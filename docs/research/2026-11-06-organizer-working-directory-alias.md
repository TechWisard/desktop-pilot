# Desktop working-directory aliases and opt-in file safety — 2026-11-06

## Starting point

Continue the unshipped organizer in PR #98, after the projection-cache work. The organizer is opt-in and must not break unrelated applications by relocating documents they expect in their working directory. Do not broaden the shipped default shortcut-only behavior.

Existing research in `docs/research/2026-10-31-organization-consent-dependencies.md` already identified shortcut argument and working-directory dependencies. There is no reason to repeat general adaptive-interface research for this particular regression.

## Evidence and risk

**Source-confirmed:** `src/organize/plan.rs` previously detected a working directory equal to the desktop only through `norm(cwd) == norm(root)`. That normalization expands environment variables, folds case and handles extended prefixes, but does not prove whether two different paths name the same directory. The organizer's *layout* already encountered real short-name/long-name aliases on Windows Server (see the October 31 organizer note). The same alias possibility applies to stored shortcut working directories.

**Primary platform contract:** Microsoft's `BY_HANDLE_FILE_INFORMATION` documentation specifies that the volume serial number together with the file-index fields identifies the same opened filesystem object, regardless of its path spelling:
https://learn.microsoft.com/en-us/windows/desktop/api/fileapi/ns-fileapi-by_handle_file_information

Microsoft also documents short (8.3) and long path spellings and notes that 8.3 generation can be disabled, so an actual short alias cannot be assumed to exist in every test environment:
https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file

**Hypothesis, not observed user harm:** An imported app could use an 8.3, alternate extended or other valid alias for the personal desktop as its working directory. If the user opts into ordinary-document organization, the program could offer and move files even though the application uses that directory. This is a safety risk identified from source, not a demonstrated user complaint or a measured productivity loss.

## Change

When ordinary files are opted in, compare local existing working directories by the same verified directory identity already used by organization. Keep the fast textual equality path; memoize repeated aliases. Reject network paths and mapped network drives before opening them to avoid unnecessary remote I/O. Do not add filesystem probes to the default shortcut-only analysis.

Run this detection both during initial preview and at Apply, which is necessary when another window imports or edits an application between the two steps. Shortcut targets and arguments are not executed.

## Reproducible integration sequence

`src/organize/consent_tests.rs::aliased_desktop_working_directory_protects_opt_in_files_at_preview_and_apply` uses production SQLite, a real Windows Shell link (not executed), an actual local directory opened via the production identity primitive, an old organization preview and normal Apply/Undo. Its intentionally distinct textual path `Desktop\\.` points at the same directory. It checks that:

1. The old preview initially proposes two document moves.
2. An imported app whose stored working directory uses the alias makes the freshly analyzed plan exclude both documents.
3. The old preview is rejected at Apply before any move, category-folder creation or recovery journal.
4. After explicitly changing the app's working directory, a new plan may be applied and undone without changing document content.

This fixture tests the path-identity mechanism without depending on 8.3 naming being enabled. It does not prove OneDrive sync behavior, physical Windows 11 icon layout or timing of real app launches.

## Acceptance boundary

Required before claiming this branch verified: standard Windows Server 2022/2025 CI (format, Rust tests, strict Clippy, build, native integration and release guards) at the exact new head. Preserve PR #98 as a draft until physical Windows 11, multi-monitor, 100%/200% GUI, worker pause/resume, interruption/Undo and redirected-desktop checks are complete. No synthetic result is a measurement of human time saved.
