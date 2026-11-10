# Organizer: literal source identity across preview, Apply, and Undo — 2026-11-06

## Real sequence and original defect

The released 0.2.122 code corrected concrete filesystem identities in the scanner and native Windows interface. Inspection of the **separate draft organizer** found old uses of the template-expanding `norm` in its analysis, path protection, Explorer polling, catalog updates, and Undo. A Desktop shortcut literally named `%USERNAME%.url` can coexist with `<Windows account name>.url`. They are different files. Under the old organizer, a manual Never rule on the second shortcut could cause the first to be omitted from the proposal, despite having distinct physical identity and a different destination. A mere UI label change would not solve it.

This is a source-confirmed failure path, not a claim of a reported field incident. The risk of conflating sources also extends to checking user-managed quick shortcuts, file dependencies, layout occupancy, and recovery catalog paths. Primary baseline: [stable 0.2.122](https://github.com/TechWisard/desktop-pilot/releases/tag/v0.2.122), [source analysis](2026-11-06-literal-filesystem-boundaries.md), and previous [organizer safety work](2026-10-31-desktop-organization.md).

## Correction and reproducible verification

- Keep environment expansion for **actual launch templates and command arguments**, not for known on-disk shortcut sources, saved managed-file paths, desktop snapshots, category root, or Undo identity.
- Reuse the released `norm_literal_path` on those already resolved paths, so source comparisons and shell-arrival checks cannot turn an existing percent-named filename into a different filename.
- Preserve conservative parent/file-ID alias fallback for genuine Explorer 8.3 path aliases. Do not equate hard links from the Public Desktop with personal Desktop icons.
- Test `organizing_literal_percent_named_shortcut_preserves_manual_sibling_and_undo`: create three different real Windows .url shortcuts, mark only the resolved sibling Never, call the actual organizer analyze/Apply/Undo paths, verify two moves, original file identities, unchanged manual sibling bytes/rule and unchanged usage history.
- Test `literal_variable_filename_is_not_a_shell_enumeration_alias` against the production Explorer polling matcher.

The scenario measures correct choices and zero unwanted edits, not real-user time savings. Verification status must be recorded from exact-commit CI; passing this local-source review alone does not certify Windows 11 native Explorer, OneDrive, multi-monitor behavior or a completed UI release.
