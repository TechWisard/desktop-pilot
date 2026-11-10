# Folder placement when Explorer cannot locate the first moving icon — 2026-11-09

## Evidence and practical scenario

The optional organizer uses Explorer's icon positions to simulate the quick-shortcut area after making category folders. The same plan is later applied by a durable journal. In the production source, both paths previously chose only the **first** moved item in each new category as its folder's position. If Explorer could not match that item's coordinate but knew the second item's position, the new folder had no target position even though a usable source cell was available. Worse, the preview silently omitted any new folder whose first source position was unknown, and could report available space without counting that future icon.

Microsoft documents `IFolderView::GetItemPosition` as returning a position on success and an HRESULT error on failure: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-getitemposition . Independent reports about desktop/Fences icon changes after reboot or refresh (https://forums.stardock.com/528482/icons-in-fences-get-rearranged-when-desktop-is-rebooted-or-refreshed) motivate avoiding optimistic assurances. These reports concern another program and do **not** establish a Desktop Pilot customer incident or quantify saved time. The failure above is a directly verified source-level inconsistency, not an observed field incident.

## Correction and regression

Both projection and journal now use `known_folder_anchor`: the first **known** position among the folder's selected moving sources. Thus a missing first coordinate does not discard the second known one. If **all** moving-source coordinates are unknown, the preview returns an unconfirmed placement instead of falsely omitting the new folder's footprint. This changes neither which files are selected/moved nor the Apply/Undo consent, identity and collision checks. The existing warning remains advisory, not a forced cancellation; Apply still checks the current Explorer topology and reports any positioning warning separately.

`created_folder_uses_first_known_source_position_and_never_promises_an_unknown_anchor` drives the production `Plan::movements`, `Plan::summary`, `Plan::projected_zone` and the shared journal anchor function with a full synthetic native Layout: first source missing, second known, both unknown, reused folder and selected singleton. Before this correction the first-known assertion fails, and an all-unknown folder is omitted by projection; afterwards both conditions are handled. The test is in-memory, so no real user desktop is touched. Full Windows builds, native Explorer/Cloud checks and scaled UI review remain required on the exact proposed head.

## Remaining acceptance

These checks do not replace physical multi-monitor/DPI, a real OneDrive account, interactive keyboard, GPU or sudden-power-loss acceptance for the unreleased 0.3.0 organizer. Do not publish or merge the entire organizer into stable `main` on this basis alone. Current stable 0.2.128, existing rules, history and local-only behavior remain unchanged.
