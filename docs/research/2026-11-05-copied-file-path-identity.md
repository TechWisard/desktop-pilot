# Copied file paths must not select unrelated catalog profiles — 2026-11-05

## Confirmed source behavior

The catalog accepts paths copied from Windows Explorer, including quotes, separator variants and names containing spaces. In the previous implementation, a complete absolute path was searched as a contiguous **substring** of a concatenation of name, category, source, target, arguments, working directory and note. Thus a path such as `C:\Users\Editor\Team Alpha\Shortcuts\editor.lnk` also matched a saved note mentioning it, or the saved source `editor.lnk.old`. Applying an Always/Never rule to such an unintended match would modify the wrong launch profile. This is a concrete source-derived defect, not a claim of observed user complaints.

Prior project evidence: [copied path search](2026-11-04-copied-path-search.md), [paths containing spaces](2026-11-04-absolute-path-spaces.md), [manual placement reasons](2026-10-30-manual-placement-explanation.md).
Primary context: [Microsoft File Explorer](https://support.microsoft.com/en-us/windows/experience/fileexplorer/file-explorer-in-windows); [Findlater & McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704). The study supports predictable user-controllable interfaces but does **not** measure this program or this defect.

## Fix and verification contract

- A complete absolute `.lnk` or `.url` filename now matches the exact saved **source** path after case/separator normalization; an unrelated note, backup suffix or launch argument cannot impersonate a source.
- A complete absolute `.exe` filename matches an exact saved executable target or direct source. Different user launch profiles sharing that exact executable may both legitimately appear, retaining their distinct arguments and rules.
- Partial folder paths and free-form keyword queries retain their broad substring/word matching. Existing filter and sort choices remain unchanged. No catalog or user data is rewritten.
- `src/ui/catalog_relevance_tests.rs` exercises real `Store`/SQLite, `rank` and `CatalogCache` using an actual normal shortcut, a stale `.lnk.old` backup whose note mentions the wanted source/target/UNC path, an executable suffix decoy, a distinct UNC shortcut, explicit Never filtering, partial directory lookup and a missing path. The pre-change substring matcher would include the backup and executable suffix decoy; the new assertions require only the relevant profiles.

This is a synthetic regression of production program logic. Before/after match-count claims must be checked against the actual CI result; no real-user task-time improvement is asserted.

## Limits and acceptance

Run normal strict Windows 2022 and 2025 checks: all Rust unit tests, native integration, formatting, Clippy, build and publication checks. The change does not canonicalize Windows 8.3 aliases, resolve symlinks, contact remote shares or merge launch identities. Test physical Windows 11 paste/search and 100–200% UI scaling separately. The filesystem organizer remains a separate draft, and this catalog change must not change its release gate.
