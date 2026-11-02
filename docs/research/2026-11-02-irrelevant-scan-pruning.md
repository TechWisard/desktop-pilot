# Ignore unrelated files during shortcut discovery — 2026-11-02

## Evidence and user impact

Continue the existing [large-catalog work](2026-10-28-large-discovery-limit.md) and [deterministic discovery investigation](2026-11-02-stable-bounded-discovery.md) without repeating searches. The existing research references Microsoft's [FindNextFileW guidance](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findnextfilew) and the usability importance of stable layouts. These references do not establish a measured slowdown in Desktop Pilot. A separate [Flow Launcher user report](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484) motivates checking custom catalog latency, but is not a Desktop Pilot incident.

**Confirmed source behavior at `95d0da0d7b117f0fe40885c312ceea71bb2c9268`:** `visit_shortcut_paths` previously collected, sorted and called `symlink_metadata` for every entry in each discovered directory, even obvious non-shortcut regular files such as `.png` or `.txt`. Thus a project folder containing thousands of unrelated documents imposed extra metadata requests and a larger sort even when the 6,000-shortcut limit was not approached.

## Bounded correction

For each entry, inspect its directory-entry type and filename. Retain possible `.lnk`/`.url` entries, directories and symlinks for the existing reparse-point validation. Skip unrelated ordinary files before sorting and `symlink_metadata`. The existing depth/budget bounds, stable name ordering, root-level shortcut precedence, folder exclusions, warnings, identities, manual settings, catalog rows and ownership safeguards remain unchanged. A folder whose name ends in `.url` is still traversed as a folder. No new configuration, network access, telemetry, persistence or deletion is introduced.

## Reproducible regression

`engine::discovery_limit_tests::unrelated_files_do_not_multiply_directory_metadata_work` runs the exact production directory visitor against a real temporary filesystem with **1,024 irrelevant files**, a visible `.URL` file and a nested `.lnk` inside a directory named `Archive.url`. A test-only counter measures the explicit `symlink_metadata` calls in the visitor. Previously the visitor performed 1,027 such calls; after the change it should perform **3** without changing discovered shortcut order or missing nested candidates. This is a synthetic count of source-level operations, **not** a benchmark of physical Windows 11 disk latency or user productivity. The existing 5,000-URL SQLite catalog, manual Always/Never and deterministic scan-bound regressions must remain green.

The change does still enumerate every entry and call `DirEntry::file_type` to distinguish directories; slower network drives remain unmeasured. No failure-first baseline CI was run specifically for this change. Validate format, locked tests, strict Clippy, build, native integration and publishing guards on the actual Windows CI before merging.

## Continued acceptance work

Measure cold-restart first useful search with genuinely large mixed directories, blocked network/cloud roots, and first-result latency at 100–200% UI scaling. Reproduce multi-week project shifts, false placements and undo with the real Store/selector. Preserve unrelated organizer PRs #98/#101.
