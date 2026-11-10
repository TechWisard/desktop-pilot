# Organizer reset confirmation after closing the dialog — 2026-11-08

## Why this matters

The organizer deliberately keeps items on the desktop when a person rejects a proposed move. Those remembered exclusions should not be reset by a stale decision from a previous visit. This is especially important when people switch projects or return after a long absence.

## Evidence and scope

**Verified source behavior before the change:** `Settings::organization_reset_confirm` started as false, and a successful reset cleared it. However, the common Close path cleared only `organization_open` and recovery consent. If a person ticked “Confirm resetting all exclusions”, closed the dialog without resetting, and returned later in the same process, the checkbox remained selected and the reset button could already be enabled. This is a local UI-state defect, not evidence of file corruption.

**External evidence, not a Desktop Pilot defect report:** Gajos et al., *Predictability and accuracy in adaptive user interfaces* (CHI 2008) studied satisfaction and use under adaptive UI changes (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). Windows users also report losing intended icon arrangements during monitor transitions (Microsoft Community, 2022: https://techcommunity.microsoft.com/discussions/windows11/windows-11-desktop-icons-keep-moving-to-another-monitor/3581539/). These support prioritizing explicit, unambiguous personal control; they do not prove that resetting exclusions was occurring in users’ sessions.

## Reproduction and correction

1. Start with an isolated real `Store`, two disposable real `.url` desktop objects and one remembered exclusion.
2. Open the production egui organizer; expand “Previously excluded objects” and tick the real reset-confirmation checkbox.
3. Click the actual Close button without resetting. Reopen the dialog in the same Settings instance.
4. Before: the reset checkbox remained true. After: it is false, no reset worker starts, remembered exclusions remain in SQLite, shortcuts retain their bytes and locations.
5. The one-line production fix revokes reset confirmation on Close, matching the existing revocation of interrupted-recovery consent.

The regression is `reopening_organizer_requires_fresh_consent_to_reset_remembered_choices` in `src/ui/organization/tests.rs`. It uses the production dialog and pointer events, not a replacement state machine. Test results from the exact commit must be checked in CI before integration. This hosted test is **not** a physical Windows 11 multi-monitor, OneDrive or power-loss acceptance test.

## Follow-up

Continue targeted interactive keyboard/escape acceptance, multi-monitor icon coordinate round-trips, and real OneDrive redirected-desktop interruption/recovery. Preserve release gate on draft PR #211. No new release is warranted solely by this narrow draft fix.
