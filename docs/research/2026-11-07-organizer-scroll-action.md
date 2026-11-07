# Organizer preview: confirmation remains visible after scrolling — 2026-11-07

## Observed behavior and evidence

The draft organizer's actual modal renderer uses a fixed header followed by a scrollable body. In the baseline source, **Apply selected moves / Remember keep choices** was inside that body *above* the searchable list. A reviewer who reaches the last page row must scroll back to find the confirmation, a particularly noticeable extra operation at 200% scaling. This is established by the source plus native, disposable-runner UI screenshots (Windows 11 ARM and Windows Server 2025, English/Russian, 100%/200%) from [run 42340409070](https://github.com/TechWisard/desktop-pilot/actions/runs/42340409070), not by user telemetry. Samples: `ru-200-organization/dialog-organization.png` and `ru-200-organization-bottom/dialog-organization.png`. Private copies of the two runner ZIP files are preserved outside Actions.

Predictability and direct user control matter in adaptive interfaces: Findlater and McGrenere, [CHI 2004](https://doi.org/10.1145/985692.985704), and Gajos et al., [CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml). These are design context, **not** evidence that this UI change saves measurable time for Desktop Pilot users.

## Small correction

Keep the *single* confirmation action alongside the modal's fixed title and Close control. Remove its scrolling duplicate rather than introducing a second destructive action. Preserve the production permission rules: disable while busy; hide on pending recovery, status error, absent or stale file-mode preview; distinguish real moves from keep-only choices; apply using the existing validation and journal paths. The preview contents, checkboxes, search filters, user exclusions, history, undo and filesystem logic are unchanged.

## Reproduction and acceptance

The existing `long_preview_uses_the_modal_wheel_at_normal_and_double_scale` fixture creates 45 real `.url` launchers and uses the actual Settings/organizer, modal wheel events and clipped egui paint shapes. Its new assertion checks that the confirmation is still painted on-screen when the last row of the first page becomes visible at both 100% and 200%. The prior code hides the action on that last-row viewport. The existing real Apply/Undo, pending-recovery, disabled-profile, confirmation and 100%/200% Close/feedback tests remain required.

Run unchanged formatting, strict Clippy, all Rust tests, native integration, Windows 11 ARM Explorer/cloud fixtures and screenshot checks. Synthetic viewport checks cannot demonstrate physical monitor scaling, keyboard focus behavior, real-user efficiency or an interrupted power-loss recovery. This change belongs only to the draft organizer integration branch until its remaining acceptance gates are satisfied.
