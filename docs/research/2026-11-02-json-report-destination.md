# Keep JSON report paths from replacing ordinary user files — 2026-11-02

## Prior evidence and limits

Continue [the existing study of user control and predictable adaptation](2026-10-26-adaptive-shortcuts.md) and [the script-replacement investigation](2026-11-02-report-script-protection.md), rather than repeating the literature search. Findlater and McGrenere's [CHI 2004 adaptive-menu evaluation](https://doi.org/10.1145/985692.985704) supports treating unexpected side effects as a usability concern, but says nothing about how often Desktop Pilot users mistype a report destination. The [Microsoft Windows naming reference](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) explains why extension-based deny lists are incomplete: extensions are not a reliable indication of a file's purpose. These are external context, not reports of incidents in this product.

## Confirmed source behavior

At main `acb72a1d10204af3ca3c46706093ff4f520cff34` (v0.2.84), the CLI accepts `--out notes.txt`, `--out photo.jpg`, `--out archive.zip` and `--out document.pdf`. The report validator rejects common executable/script/database/shortcut formats but intentionally cannot enumerate every user document format. `report::write` writes JSON bytes to an adjacent temporary file, then calls `MoveFileExW` with replacement enabled. A typo selecting an existing ordinary document therefore risks irreversibly replacing its contents before a user notices; this is established by control flow, not an observed real-world failure rate.

The CLI documentation explicitly describes `--out` as a JSON report. The smallest consistent rule is to require its destination filename to end in `.json` (case-insensitively). Keep existing permitted JSON report overwrite behavior, reject non-JSON paths before opening a profile or applying/undoing placement, and leave the separate internal `last-error.txt` diagnostic untouched. Do not infer content type from arbitrary filenames, do not add another permission, UI setting, service, dependency or telemetry.

## Production-path regression and related scenarios

`cli::tests::json_report_option_rejects_document_and_backup_paths` covers ten representative paths, including a missing extension, deceptive `.json.bak`, and uppercase allowed `.JSON`. The native `--self-test` invokes the **actual built executable** as `--inspect --home <absent profile> --out <existing document>` for four different synthetic documents and asserts all commands fail, original bytes remain identical, and no new profile is created. The existing native suite already verifies successful `.json` reporting, blocked SQLite replacement, hard-linked log safety, and file-changing rollback. Those tests and this new case should run on both existing Windows CI versions, not a simplified simulated CLI. No failure frequency, user-time savings or real Windows 11 interaction outcome is claimed.

## Remaining verification

Run the existing Windows Server 2022/2025 locked Rust tests, strict lint, format, build, native integration and release guards. Check related manual Apply/Undo scenarios before release. A report still intentionally replaces an existing *JSON* file at the provided path; fully preventing mistaken selection of another program's JSON settings would require a separately designed opt-in overwrite policy and compatibility study. Test the real Windows 11 UI and unusual filesystem permissions before asserting full coverage.
