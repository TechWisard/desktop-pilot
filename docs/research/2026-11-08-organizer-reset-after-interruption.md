# Reset confirmation across recovery and fresh desktop review — 2026-11-08

## Source observation

The organizer has a deliberately explicit checkbox before clearing previously remembered keep-on-desktop choices. The earlier fix revoked this consent on **Close**. Inspection of the production `Settings::organization_dialog` showed another path: a second Settings process can set `organization.active.v1` while the first modal stays open. That temporarily hides reset controls. If the operation is then recovered elsewhere, the first modal previously retained the checked reset flag. Similarly, a new analysis cleared the search and preview but not an earlier reset confirmation. The user could return to a newly changed review with the reset button already enabled. This is a code-confirmed UI-state defect; no user data loss incident is claimed.

Microsoft's [Windows confirmation guidance](https://learn.microsoft.com/en-us/windows/win32/uxguide/mess-confirm) recommends a safe default for consequential actions and user-initiated confirmation. That older UI guidance is design context, not Desktop Pilot defect evidence.

## Reproducible production-path scenario

The Windows/SQLite/egui regression `pending_organization_and_new_analysis_require_new_reset_consent` uses the actual Settings dialog and Store: (1) save an exclusion; (2) check reset without performing it; (3) simulate a different instance entering recovery by setting its real persistent active-journal marker; (4) render the actual recovery UI; (5) clear the marker, render again; (6) verify the earlier reset authorization is absent; (7) deliberately check reset again, request a new real organizer analysis, and verify that consent is revoked on this second context change.

The fixture checks that neither original Windows `.url` shortcut moves, the stored exclusion remains unchanged, no background job is started by the recovery-only frames, and the operation marker is clear. This tests UI state against the production status/analysis code, not a separate mock of its decision rules. The transient journal marker is synthetic; it does not substitute for an actual interrupted filesystem operation.

## Decision and comparison

Before: consent survived recovery and a fresh analysis until Close or a successful reset. After: an interrupted/error status and a newly requested analysis each clear consent. Reconfirming remains an explicit choice. The regression asserts stability and absence of filesystem changes, not measured human time savings. Physical Windows 11, OneDrive, multi-display and power-interruption acceptance remain required before the organizer is released.
