# Organization: newer manual choices and launch dependencies

This continues the read-only plan / explicit Apply / recoverable Undo work in PR #98. No release is claimed by this note.

## Confirmed regressions on the real implementation

Test-only commit: `9991f7b17308dc0612e9a1163973ac43f1c494fe`.
Windows CI: https://github.com/TechWisard/desktop-pilot/actions/runs/41208348463

The Windows Server 2022 job compiled and ran 373 Rust tests: 370 passed and the three new tests failed at their intended behavioral assertions. The same run's formatting check also rejected wrapping in the new test file; that separate authoring error did not prevent the behavioral test step from executing. Both Windows jobs were red. No acceptance check was weakened and no release was published.

1. An old preview remained open. A newer plan saved Keep for Alpha, moved Beta and Gamma, and was then undone. Applying the old preview moved all three objects, ignoring the later Keep choice.
2. An actual Windows `.lnk` opened an ordinary document using its command-line arguments. The optional file organizer offered to move that document, breaking the saved argument path.
3. The same dependency was introduced after preview. Apply moved both documents instead of rejecting the outdated plan before mutation.

The fixes reload saved Keep identities under the changes lock at Apply, and include arguments in the conservative dependency checks during both analysis and Apply. Matching a literal path or filename is a reason to leave an object in place, not a claim that a target application's argument grammar was fully parsed. Arguments are never executed. If any local shortcut's metadata cannot be read, ordinary-file grouping is withheld; independently recognized shortcut grouping may still proceed.

Additional tests cover relative/case-varied argument spelling, malformed shortcut metadata, restoration of availability after removing the obstruction, no overwrites, actual Undo, preserved catalog rows and explicit reset of Keep choices.

## Limits and acceptance

This does not infer arbitrary document-internal links, dependencies stored outside the known catalog, URL-encoded argument paths, or the intent of an external application. Ordinary files remain an explicit opt-in, with a preview and journaled Undo; unknown project/code/spreadsheet types are not moved. No application is launched and no remote target is resolved by the analyzer.

The post-fix run must be recorded after it completes. Existing full compilation, lint, native runtime and UI acceptance requirements still apply. These isolated filesystem/SQLite scenarios are not physical Windows 11 validation or a measurement of user time savings.
