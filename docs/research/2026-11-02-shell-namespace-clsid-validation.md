# Reject malformed Shell namespace CLSIDs without displacing working shortcuts — 2026-11-02

## Evidence and practical scenario

Continue [incomplete protocol shortcut research](2026-11-02-empty-protocol-shortcuts.md), the [adaptive-interface evidence](2026-10-26-adaptive-shortcuts.md), and [predictable catalog discovery](2026-11-02-stable-bounded-discovery.md). Do not count a repeated literature search as a new finding.

**Primary operating-system source:** Microsoft's [namespace extension documentation](https://learn.microsoft.com/en-us/windows/win32/shell/nse-junction) describes Explorer paths like `::{Extension CLSID}`; its [rooted shortcut examples](https://learn.microsoft.com/en-us/windows/win32/shell/how-to-use-a-shortcut-file-to-open-a-rooted-view) include nested `::{Folder CLSID}\\::{Extension CLSID}` components. A CLSID uses the standard 8-4-4-4-12 hexadecimal identifier form. The sources establish path *syntax*, not whether any arbitrary registered CLSID is installed or launchable on this particular Windows machine.

A [February–March 2026 Fences user report](https://forums.stardock.com/540842/fences-6-unilaterally-rearrange-desktop) describes unexpectedly reorganized icons and difficulty trusting the arrangement of a **different product**. Gajos et al. ([CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml)) found improved satisfaction with greater accuracy and predictability in adaptive interfaces. Neither source demonstrates Desktop Pilot incidence or measured benefit.

**Confirmed source-path defect:** the existing `native::valid_target` considered a `::{...}` target valid whenever any closing brace appeared after the opening prefix. `::{}`, `::{not-a-guid}`, a truncated GUID, and `::{valid-GUID}\\::{}` therefore passed validity despite not containing a valid CLSID for every namespace segment. A saved historical seed can then make the invalid .url eligible for a limited quick slot. This is a source-level hazard; user incidence is unknown.

## Narrow correction

Validate the first Shell namespace component as a hexadecimal CLSID with canonical separators; validate any immediately chained namespace component the same way. Allow valid namespace roots, nested roots, and extension-defined child object names after a valid CLSID, without trying to resolve the installed shell extension during catalog scanning. Do not alter Steam, HTTP, normal Shell paths, ranking, user policy, file contents, stored history, layout, or Undo. No telemetry, network call, new setting, or new dependency.

## Reproduction and acceptance

Extend `native::tests::incomplete_protocol_shortcuts_cannot_displace_a_working_choice`: create real malformed .url files, parse with production `native::read_link`, give each invalid target a higher persisted seed than a working HTTPS URL, and use production `engine::preview_loaded` to verify the working candidate keeps the sole slot. Assert the original .url bytes are unchanged, and include valid single and nested CLSID controls. Without the correction the new `::{}` and `::{not-a-guid}` expectations fail.

Run unchanged Windows Server 2022 and 2025 format, locked Rust tests, strict Clippy, build, native integration and publication guards. CI does not establish that a registered Shell extension launches correctly in Explorer on a physical Windows 11 installation. Interactive GUI scaling and removable-volume testing remain outside this change. Avoid releasing until the complete product/release gates succeed.

## Continued work

- Preserve unrelated in-flight UI organizer PRs #98 and #101.
- Investigate exact real Windows Shell namespace behavior and long-lived mixed app sessions with controlled local fixtures, not presumed real-user timings.
- Avoid duplicate complete binaries or diagnostic artifacts in GitHub Actions; preserve unique results before cleanup.
