# Preserve literal-percent files referenced by shortcut arguments — 2026-11-07

## Evidence and user scenario

The desktop organizer only moves ordinary files after explicit opt-in. Its dependency pass reads the actual saved Windows Shell-link arguments from `native::read_link`, in addition to the catalog, before offering a file for relocation. Microsoft documents these stored arguments in [IShellLink::GetArguments](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getarguments). Windows also permits percent signs in ordinary filenames; command-processor [environment substitution](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd) is a separate operation.

The source audit found that `Dependencies::record` normalized **all** shortcut arguments with `model::norm`, which expands `%SystemRoot%` and similar tokens. That is correct for a deliberate environment template but loses the *literal* spelling of an existing file named `Important%SystemRoot%.txt`. The following `Dependencies::check` compares against the actual Desktop path and leaf name. Consequently, the organizer could offer a file which a real shortcut still explicitly names, breaking that launch after Apply.

The constructed case is a source-confirmed safety defect and a test hypothesis, not a reported user incident or measured human time saving. It complements, rather than repeats, [the literal sidecar investigation](2026-11-07-literal-photo-sidecars.md). The existing [interface-research record](2026-10-26-adaptive-shortcuts.md) identifies preserving manual control and avoiding surprising reconfiguration as priorities.

## Narrow correction

Store **both** interpretations of a saved argument string in the conservative dependency index: the literal spelling and the environment-expanded spelling. Neither is executed, and the organizer does not infer application argument grammar. If either representation names a proposed file, keep that file on the Desktop. The tradeoff is occasional false-positive protection, safer than silently breaking a saved launch. Existing launch behavior, user settings, files, rules and local history are unchanged.

## Production-path reproduction

The Windows regression `literal_percent_shortcut_argument_keeps_the_exact_document_during_organization` creates a real `.lnk` with `native::make_link`, a literal-percent ordinary file and two independent documents in a disposable Desktop, then calls the production SQLite-backed `analyze`, `apply` and `undo` functions. Assertions cover the stale-plan rejection, explicit skip reason, independent-file moves, byte-preservation for the linked file and shortcut, return of the two unrelated files, catalog stability and absence of a pending recovery journal.

**Synthetic before/after criterion:** the old argument index contains only the expanded form and misses this file; the corrected index retains the literal form and excludes it from movement. The independent documents must remain movable. Related acceptance includes the existing normal-argument, working-directory, sidecar, stale-plan and interruption regressions.

## Release gate

Run unchanged `cargo fmt --check`, `cargo test --locked`, strict Clippy, build, native integration and public Windows acceptance checks. Do not merge or release the larger organizer solely on this fixture: live Windows 11 GUI review, OneDrive and interruption acceptance remain separate gates. No telemetry, external services or user-file migration are introduced.
