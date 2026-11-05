# Public Desktop hard links must remain occupied in projected layouts — 2026-11-05

## Confirmed source-level defect

The draft organizer estimates a free quick-shortcut zone after selected originals would move into category folders. It removes icons by exact path or by leaf name plus filesystem ID, to accommodate Explorer short/long 8.3 aliases. On NTFS, however, a second hard link in **Public Desktop** has the same file ID and leaf name but a different parent directory. The old projection incorrectly removes *both* icons, describing an occupied cell as free and potentially suggesting a placement that conflicts with an icon the user never chose to move.

This was established by reading the production `Plan::projected_zone` and the previous projection test. It is a reproducible synthetic layout failure, **not** an observed incident on a real user's desktop.

## Evidence and expected behavior

- Microsoft explains that multiple NTFS hard-link names can reference one file: https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions
- Microsoft documents file identity and volume serial in `BY_HANDLE_FILE_INFORMATION`: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information
- Related PowerToys reports document sensitivity to incorrect window placement and changing monitor layouts, not this exact hard-link bug: https://github.com/microsoft/PowerToys/issues/37634
- Existing project investigation `docs/research/2026-11-05-organizer-double-location-hardlink.md` covers double-location Undo and Acceptance separately. These are different production paths; green Undo tests did not verify projection.

A projected icon is removable through the fallback only if its file identity **and parent directory identity** match the selected original's desktop root. Failing to inspect the parent conservatively retains occupancy. Exact canonical path matching remains unchanged, as do file moves, SQLite journal, manual choices, cancellation, telemetry policy and actual native placement verification.

## Reproducible regression and synthetic comparison

`projected_space_retains_public_desktop_hardlinks_and_recognizes_local_aliases` in `src/organize/tests.rs` uses real Windows files and NTFS hard-link identities against production `Plan::projected_zone`, with a synthetic three-cell Explorer layout.

1. Three website shortcuts occupy cells 0, 1, 2. Two selected shortcuts would move to a new folder at cell 1; the projected free cell is 2.
2. The selected `Gamma.url` is reported through a dot-segment alias of its real parent directory. Projection must still remove that one view entry and propose cell 2.
3. Another `Gamma.url` hard link in a different `Public` directory occupies cell 2. Its file ID is identical to Gamma's; its parent is different. **Before** the fix, the projection would still report cell 2 free. **After**, there is no free cell.
4. The physical files are unchanged by projection. The fixture is deleted afterward.

This checks the real projection method and Windows file identity with synthetic icon coordinates. It does not measure human time savings or establish actual Explorer coordinate behavior.

## Next acceptance

Run unchanged Windows 2022/2025 CI with Rust tests, formatting, strict Clippy, build, native integration and publication guards. The organizer PR remains a draft pending interactive Windows 11, 100–200% GUI, redirected/OneDrive Desktop, real Explorer positioning, and interruption/recovery acceptance. Do not publish this as a finished organizer release.
