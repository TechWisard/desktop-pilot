# Ordinary and extended Windows paths in catalog search — 2026-11-05

## Source-derived defect and evidence

The full-file-path search matches saved source/target by lowercase text and slash conversion, despite the existing model::norm helper treating ordinary and extended Win32 paths as the same file identity. A copied C:\Users\Editor\Team Alpha\editor.lnk cannot locate a saved \\?\C:\Users\Editor\Team Alpha\editor.lnk; likewise a copied \\server\share\folder\file.url cannot find \\?\UNC\server\share\folder\file.url. This is confirmed from production source code, not a reported real-user incident.

[Microsoft's Win32 namespace documentation](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) distinguishes ordinary, extended drive and UNC forms. [Gajos et al., CHI 2008](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) supports accurate predictable adaptive interfaces generally, not measured Desktop Pilot outcomes. Prior regression: [copied-file-path identity](2026-11-05-copied-file-path-identity.md).

## Fix and executable scenario

Normalize only complete absolute .lnk/.url/.exe queries and their exact source/target fields with the production model::norm helper. Do not change free-text or partial-directory matching, user data, policy priorities, launch identities or file bytes. Do not collapse unrelated volume-GUID/GLOBALROOT device namespaces.

The copied_paths_resolve_extended_drive_and_unc_forms_without_matching_backups test stores three real SQLite App records and calls production rank and CatalogCache. It checks ordinary and extended drive/UNC queries, quoted paths, executable lookup, backup/notes that mention a path, Never filtering, a missing path, free-text search and unchanged records. Before the fix the ordinary aliases fail; the intended outcome is one exact launch profile for each query. Synthetic correctness is not measured time saved.

## Verification boundaries

Require normal strict Windows 2022/2025 CI: fmt, all locked Rust tests, Clippy -D warnings, build, native integration and release guards. Windows 11 visual interaction at 100-200% scaling, actual remote share availability, 8.3 aliases and filesystem symlinks remain unverified. The draft desktop organizer stays separate; no release without verified assets.
