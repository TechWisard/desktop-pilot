# Keep multipart archive downloads together

## Scope and research

Continue the explicit-preview and conservative-dependency contract in
`2026-10-31-desktop-organization.md`; do not add another cleanup setting or
silently opt ordinary files in. The existing CHI research record remains the
basis for predictability and user control, not a claim of measured time saved:
https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/

Primary format/application documentation reviewed on 2026-11-08:

- WinZip documents `.z01`, `.z02`, ... plus the final `.zip`, including numbers
  above 99: https://kb.winzip.com/en/130798
- RARLAB documents `.part1.rar`, `.part2.rar` and keeping volumes in one folder:
  https://www.rarlab.com/rar_file.htm
- WinZip documents old `.rar` / `.r00` / `.r01` and numeric `.000.rar` /
  `.001.rar` conventions: https://kb.winzip.com/en/125950
- Bandizip explicitly diagnoses missing volumes when parts are in different
  folders: https://www.bandisoft.com/bandizip.mac/help/split-archive-error2/

A first-person 7-Zip forum report describes split ZIP backups and unsuccessful
extraction experiments. It establishes a real multipart-backup workflow, not
that Desktop Pilot caused the reported problem, nor the behavior of current
7-Zip: https://sourceforge.net/p/sevenzip/discussion/45797/thread/0c9e3981/

## Product defect and narrow change

The optional ordinary-file plan admits `.zip` and `.rar`, but rejects `.z01`
and `.r00`. The previous associated-file index protected only media sidecars.
Consequently a split archive's admitted member could move while its other
parts stayed on the desktop. RAR volumes ending in `.rar` could also be split
by a manual deselection or a later-arriving download.

Extend the existing associated-file guard, not the rename engine. Index the
bounded directory snapshot once. Protect a `.zip` with an exact-basename
numbered `.zNN` companion, a `.rar` with an `.rNN` companion, explicit
`.partN.rar` members even while a download is incomplete, and numeric `.NNN.rar`
sets when distinct equal-width siblings exist. Match case-insensitively while
preserving literal percent signs and Unicode paths. Reuse the existing
localized associated-file explanation. No archive contents are opened, no
external unpacker runs, and no new dependency, telemetry or permission is added.

Both Analyze and the existing pre-Apply dependency rebuild use this guard.
A new companion invalidates an old plan before folders, journals or file moves
are created. Independent archives remain eligible and keep their normal Undo.

## Reproduction and regression contract

`src/organize/archive_set_tests.rs` runs the real analyzer, SQLite store,
Windows identity-checked rename and Undo in disposable local directories.
The three regression scenarios were first committed against the unchanged
implementation at `f07705a2fb17ad99d0c388d39a1154bb5077053c` (Windows run
42487087588). Record actual outcomes from that run, not inferred success.

1. Four multipart conventions plus two independent archives and an important
   manually pinned shortcut. Assert exactly two eligible moves, read-only
   analysis, preserved bytes/file identities, preserved usage and manual rule,
   and durable Undo through a newly opened SQLite connection.
2. A ZIP, old-style RAR or numeric RAR companion arrives after preview. Assert
   rejection before any move, no empty category folder, no new Undo/pending
   journal, intact saved exclusions, and safe reanalysis.
3. An explicitly numbered partial RAR download, a manual Keep choice and two
   independent archives. Another volume arrives before Apply; the independent
   moves and Undo must not disturb either archive volume or the remembered Keep.

Name-index checks cover exact matching, misleading suffixes, numeric-width
ambiguity, duplicate input and 6000 synthetic paths without file I/O. Existing
media-sidecar regressions remain unchanged.

The fixtures contain opaque bytes with documented volume filenames; they do
not test compression/decompression or claim that every matching name really is
an archive. The before/after metric is unwanted proposed/applied file moves,
not measured human time savings.

## Acceptance and remaining limits

Run the unchanged full Windows checks on the exact final head: format, locked
Rust tests, strict Clippy, build, native integration, Explorer, cloud and UI
fixtures. Do not merge while those checks fail. CI evidence and the final SHA
belong in the accompanying PR discussion; this file does not predeclare them.

The guard is intentionally conservative about explicit volume names. It does
not inspect archive headers, recognize arbitrary renamed/encrypted volume
schemes, or identify a lone ambiguously numeric archive before a sibling
exists. It does not promise atomicity against a file arriving after the final
filesystem snapshot. Physical multi-monitor Windows 11, a real OneDrive
account, interactive keyboard/accessibility and physical power-interruption
acceptance for the whole organizer remain open. This is an unreleased organizer
change, not permission to ship 0.3.0 or modify stable main.
