# Overlapping Known Folder subroots and shared shortcut capacity — 2026-11-02

## Evidence and scope

Continue the [root-deduplication regression](2026-11-02-duplicate-known-folder-roots.md) rather than restarting the Windows Known Folder or adaptive UI literature searches. Microsoft's [Known Folder API documentation](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders) confirms that per-user and common folders can be redirected, including to arbitrary local or network locations. [CHI 2008 research](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) establishes that accuracy and predictability influence satisfaction in adaptive interfaces, **not** that this particular issue affects real users.

**Confirmed code-path defect:** prior scan-root deduplication only skipped roots whose normalized path strings were identical. An overlapping (ancestor and descendant) pair still traversed the same shortcut twice. The shared capacity was charged *before* the final duplicate removal. Under the configured 6,000-file cap, a later unrelated Start Menu shortcut could be omitted even though fewer than 6,000 distinct sources had been discovered. No estimate of incidence or user time saved is made.

## Reproduction

The production visitor and root collector are invoked by `overlapping_known_folder_subroots_count_only_distinct_shortcuts`. Real Windows directories contain three actual `.url` files:

- User Desktop: `Alpha.url`, plus `Shared/Beta.url`;
- another Known Folder root: that same nested `Desktop/Shared`;
- Start Menu root: distinct `Gamma.url`.

With a **three-source** synthetic budget, the old collector counted `Beta.url` twice, stopped before `Gamma.url`, and only then deduplicated the list. With a shared set of normalized visited shortcut paths, the collector returns **all three distinct files**, exactly as if the duplicate nested root had not been configured. All three source byte sequences are checked unchanged. A bounded-capacity warning is still emitted. The existing exact-root, order, priority, reparse-point, large catalog and manual-policy regressions remain applicable.

## Scope and safeguards

The change deduplicates visited **shortcut paths**, not executable targets: two shortcuts with different paths but the same target remain separate discovery candidates until existing logical-identity handling. It never follows reparse points, removes files, rewrites original sources, lifts the scan limit, modifies policy, or adds telemetry. Normalized path comparison does **not** resolve hard links, volume aliases or remote filesystem identities; this guard is for lexical overlap, not a proof of complete filesystem alias equivalence.

## Validation and remaining work

The shared visitor is production code; the fixture is a synthetic, reproducible Windows filesystem case, not a user study. Run unchanged formatting, locked Rust suite, strict Clippy, Windows x64 build, native integration and release guards on Windows Server 2022 and 2025. Physical Windows 11, redirected OneDrive/network roots, unusually slow enumerations, Explorer UI at 100–200%, and actual human task times remain unverified. Do not merge the unrelated desktop-organization draft PRs #98/#101 without their separate acceptance.
