# Leading UTF-8 BOM in pasted shortcut lists, and accurate catalog instructions (2026-10-29)

## Starting point and evidence

Baseline: released v0.2.55 at `96d8d8460b531e435de2b193ad4f0217afa2d2b7`; published Windows 2022/2025 CI run 40890659838 successful and no open pull requests at inspection. The repository is public, so its existing complete Windows Actions gates remain authoritative. A separate public test repository is unnecessary.

The previous stage, `docs/research/2026-10-29-partial-pasted-import.md` / PR #59, made the real Settings worker accept valid shortcut selections despite malformed or relative neighbors. Its initial text splitter still treats a UTF-8 byte-order mark U+FEFF at the very start of pasted text as part of the first filesystem path. Windows PathBuf then sees a leading character before the drive letter; a valid absolute, quoted .url/.lnk file is rejected as a relative path, even though other lines proceed. This is directly established by inspecting the production parser; no external customer report or real-user time saving is claimed.

The [Unicode BOM FAQ](https://www.unicode.org/faq/utf_bom.html) distinguishes the signature at the beginning of a text stream from characters inside the text. Preserve any U+FEFF found later instead of silently rewriting user paths. This is a precise import-integrity issue, not an invitation to broadly reinterpret unknown file formats.

## Reproducible production-path sequence

New `src/ui/import_ownership_tests.rs::pasted_utf8_signature_does_not_discard_a_valid_quoted_shortcut` drives the real pasted-list splitter, async Settings import worker, native Windows URL parser, SQLite catalog, saved manually protected shortcut, prior usage, preview and Undo. The batch consists of a valid absolute file with an initial U+FEFF and Windows quotes, followed by a deliberately invalid relative .lnk path. Required outcome: the valid file imports once, the invalid line produces its own explanatory error, original bytes and manual rules/history/positions remain untouched, preview proposes no undesired movement, no-op reimport stays unchanged and Undo preserves originals.

The correction removes *only* the initial U+FEFF character of the complete pasted text. No U+FEFF on a later line or embedded in a path is stripped; the existing worker rejects those malformed candidates individually. Per-file validation, SQLite atomicity, duplicate identity matching and all protections from v0.2.55 remain in place. No new dependencies, user preferences, background services, telemetry or profile migrations.

## Documentation correctness

The engine currently defines `MAX_DISCOVERED_SHORTCUTS = 6_000`, while the bilingual guides still promised only 3,000. The guides also described the post-import view as “recently added”, although the real UI uses “Last import” and displays re-selected existing entries. Both guides now describe the effective 6,000-entry ceiling and honest filter behavior. A read-only documentation guard derives the numeric limit from the production scanner constant and checks both languages. No scanner behavior is changed.

## Validation and remaining limits

The regression was introduced against unchanged production behavior before the correction. The standard Windows CI must prove the failure is behavioral rather than formatting or compilation, and separately pass the fixed candidate, PR, main and release jobs. Required checks include locked Rust tests and build, strict Clippy, native integration, README/guide consistency and publication guards, plus version/tag/asset confirmation. Full UI smoke render, physical Windows 11 at 200% DPI, actual SMB/cloud drive delays and human task-time measurements remain outside this fixture and are tracked in #22.

Independent analogous-product and adaptive-interface research is already accumulated in `docs/research/2026-10-26-adaptive-shortcuts.md` and the prior import research. It is not repeated here.
