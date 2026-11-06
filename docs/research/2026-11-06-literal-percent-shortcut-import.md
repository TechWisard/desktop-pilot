# Literal percent-token names in pasted shortcut paths — 2026-11-06

## Evidence and scope

**Source-confirmed issue, not a user-reported Desktop Pilot incident.** The Apps import path parser intentionally expands an environment variable only when the supplied spelling is not already an absolute path. Its subsequent duplicate filter nevertheless called `model::norm`, which expands `%NAME%` again. Windows allows a directory literally named `%USERNAME%`; that entry and a directory whose name is the current username can both contain distinct `Editor.url` files. The original filter treated them as the same path, silently skipping one import. The direct native shortcut parser does not have this ambiguity.

Microsoft's [Windows file naming conventions](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) explain literal filename components, and the [cmd environment substitution documentation](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd) distinguishes explicit `%USERNAME%` substitution in commands. This distinction is also why filesystem paths chosen directly from Explorer must not be reinterpreted as command strings.

## Change

Keep existing `norm` behavior for callers that intentionally accept unexpanded environment roots. Introduce `norm_literal_path` with the same Windows drive/UNC/case rules but **no variable expansion**, and use it solely when deduplicating already parsed concrete imported paths. Do not modify originals, import rules, saved profile selection, existing search ranking, or the background worker.

## Reproducible real-logic regression

Create two different `.url` files in separate real Windows directories, `%USERNAME%` and its actual expanded username, with the same `Editor.url` leaf but different HTTP destinations. Paste both absolute paths. Before the change, duplicate filtering returned **1** path; after the change, it returns **2**. Verify both with the actual Windows shortcut reader and compare their original bytes after parsing. A second regression additionally goes through the real asynchronous Settings import job, COM setup, SQLite transaction, preserves an existing manually owned shortcut and its history, and verifies a repeat import makes no extra changes. The existing quoted/UNC/case-insensitive duplicate tests remain applicable.

This is an import-candidate count in a synthetic fixture, **not evidence of time saved** by actual users. Source and test need the normal Windows 2022/2025 strict CI. Live Windows 11 GUI, OneDrive, scaling, and the separate draft desktop organizer still require physical acceptance.

## Follow-up

Audit remaining `norm` call sites to distinguish command-style unexpanded paths from literal filesystem paths, especially catalog source reconciliation and already imported originals. Do not alter global normalization of legacy saved profiles without real-file identity tests and migration coverage.
