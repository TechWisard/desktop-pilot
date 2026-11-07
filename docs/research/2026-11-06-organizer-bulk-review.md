# Reviewing large desktop organization plans without repeated clicks (2026-11-06)

## Evidence and scope

Source inspection at draft organizer head `04b0ac31443898e433e6b509bf235f00c887c105`: `src/organize/mod.rs` bounds analysis to 6000 objects; `src/ui/organization.rs` presents only 40 items per page, defaults eligible candidates to selected, and previously offered one checkbox per item but no global reversal. To keep one shortcut out of a 6000-item proposal, the user would have needed to clear thousands of individual selections. This is a code-derived usability defect, **not** a complaint from an observed Desktop Pilot customer.

HCI background already preserved in `docs/research/2026-10-26-adaptive-shortcuts.md`: Findlater/McGrenere (CHI 2004), Gajos et al. (CHI 2008) and ephemeral adaptation research emphasize control, predictability and stable user choices. Do not repeat the broad search or claim their studies specifically evaluated this feature. Microsoft's documented Select All command is a conventional bulk-selection pattern (https://support.microsoft.com/en-us/windows/accessibility/keyboard-shortcuts-in-windows); its existence does not prove it is optimal for this workflow.

## Change and production-path verification

A single click can now deselect all items or select all items **across every page and search filter**. A visible `Selected: n / total` count and RU/EN explanation make the scope explicit. Unselected items remain on the desktop, and the established Apply/Undo, lock, revalidation, journal, SQLite and local-only privacy boundaries remain unchanged. Buttons are disabled while a background action is running or when no state change is possible. Per-item checkbox control remains for fine-grained decisions.

The regression `src/ui/organization/tests.rs::one_bulk_choice_changes_the_entire_real_plan_even_with_hidden_rows` renders real egui dialog buttons and uses the existing Windows fixture with two real `.url` files, the production organizer plan and SQLite store. It hides both plan rows by search, deselects them globally, selects them again, hides one, deselects both, and applies the existing keep-only workflow. It asserts both contents unchanged, no category folder and no Undo journal, and verifies a new analysis excludes the remembered items. Existing 6000-item pagination and Apply/Undo tests remain enabled.

**Synthetic interaction bound, not a human timing result:** an all-deselected state requires one bulk click rather than up to 6000 individual checkbox actions. Actual task-time savings, real Explorer icon arrangement and cognitive usefulness have not been measured.

## Acceptance and remaining limitations

Keep this candidate on a dedicated branch targeting draft PR #98. Run the repository's existing Windows Server 2022/2025 strict checks: `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integration and publication guards. Do not merge to `main` or cut a release solely from this result. Physical Windows 11, scale 100%/200%, multi-monitor and OneDrive interrupted-operation acceptance for the full organizer remains outstanding. Never create or claim telemetry.
