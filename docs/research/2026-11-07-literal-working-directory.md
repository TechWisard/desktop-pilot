# Literal-percent working folders: preserve files referenced by a saved launch — 2026-11-07

## Scenario and observed source defect

The optional desktop organizer can group ordinary documents after the user explicitly selects that option. A Windows Shell link separately stores its executable, arguments, and working directory. When the executable's working directory is the current personal Desktop, moving its data files can silently break relative-path workflows even though the executable itself is unchanged.

The production `Dependencies::record` working-directory guard formerly called `expand_env` **before** comparing the saved directory with the Desktop and before resolving its local directory identity. A valid directory named `Work%USERNAME%` could therefore be transformed into a different path and missed. The link target and icon had already gained dual literal/expanded protection, but its working-directory check did not. This is a source-confirmed defect, not an observed real-user incident.

Primary technical reference: [Microsoft IShellLink::GetWorkingDirectory](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getworkingdirectory) and [Shell links](https://learn.microsoft.com/en-us/windows/win32/shell/links).

Existing human-interface evidence was retained rather than duplicating the literature survey: [CHI 2008 predictability and accuracy study](https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/) found stronger satisfaction when adaptive choices were predictable and accurate. Its participants did not use this product and the experiment does not prove time savings here. See [adaptive-shortcuts research](2026-10-26-adaptive-shortcuts.md) and [desktop organization contract](2026-10-31-desktop-organization.md).

## Correction and reproducible production-path scenario

Check **both** saved literal and environment-expanded working-directory spellings. Exact Desktop equality is cheap. For a different textual spelling, resolve only a local directory and compare its existing filesystem identity to the Desktop root. Stop immediately when a match is established. Do not run links, access remote paths or move the original Desktop. The extra identity probe is limited to the existing opt-in ordinary-file pass; the shortcut-only default is unchanged.

The regression `literal_percent_desktop_working_directory_preserves_opt_in_documents` uses a real Shell-link COM write and read, SQLite-backed stored catalog, literal `Work%USERNAME%` Desktop directory, two unrelated documents and the real `analyze` / `apply` / `status` paths. It checks two sequential states:

1. Before the link is added, both files are valid optional organization candidates.
2. Once a saved link uses that literal Desktop as its working directory, the refreshed analyzer explicitly excludes both documents. A stale preview cannot apply the old move; file bytes, catalog, folders and undo journal remain unchanged.

This is a safety improvement at the cost of potentially retaining more files when an application's working-directory dependency is ambiguous. It is deliberately not a new automatic file-move feature.

## Required validation and remaining acceptance

Use the existing public Windows workflow unchanged: locked Rust tests, `cargo fmt --check`, Clippy with warnings denied, build, native integration and disposable Explorer/UI acceptance. A passing isolated test only proves this synthetic sequence on the tested runner; live user Windows 11 desktop, interactive 100%/200% GUI review, cloud redirects, interruption recovery and real-life time-to-benefit still require independent acceptance. Keep the organizer integration in draft until those gates are resolved. No release should be published for this narrow fix.
