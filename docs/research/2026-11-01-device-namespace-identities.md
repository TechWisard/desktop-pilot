# Preserve Windows device-namespace shortcut identities — 2026-11-01

## Decision and evidence

A desktop shortcut can refer to a removable volume by its persistent volume GUID instead of an ordinary drive letter. Windows distinguishes extended drive paths (\\?\C:\Tools\App.exe), extended UNC paths (\\?\UNC\Server\Share\App.exe), volume paths (\\?\Volume{GUID}\Tools\App.exe) and the global NT namespace (\\?\GLOBALROOT\Device\...). Microsoft documents why the prefixes select distinct namespaces:

- [Naming Files, Paths, and Namespaces](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) (primary Windows specification).
- [Naming a Volume](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-volume) (volume GUID paths and mount points).
- [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) experimentally found that users often prefer direct control over automatically shifting interface items. The previously collected [adaptive-interface evidence](2026-10-26-adaptive-shortcuts.md) argues for preserving manually selected shortcuts, not for another presentation feature.

These sources establish namespace semantics and an interface-design motivation. **They do not establish a Desktop Pilot user incident or time savings.** The catalog collision is confirmed by the production source and failure-first tests. The affected workflow (rare project tools on removable/volume-GUID paths, explicit Always/Never choices and relative launch profiles) is a hypothesis about real user impact, not a measured prevalence claim.

## Source-derived defect and production-path reproduction

At main a5c6aab4401e29dfc8017907e564aa32e0cffcb9, model::norm preserved the special extended UNC case but stripped the \\?\ prefix **from all other paths**, not just conventional absolute drive paths. That made a volume-GUID path indistinguishable from an unrelated relative string (Volume{GUID}\Tools\App.exe). App::identity and App::assign_id use this normalized target; Store::put_app upserts the catalog keyed by the resulting ID. A second legitimate launch profile could overwrite the first and its saved manual rule. A similar collision exists for GLOBALROOT.

The regression in src/model/device_namespace_tests.rs exercises **the actual normalization, App ID derivation and SQLite Store**, rather than a separate selection model:

1. Compare volume-GUID and GLOBALROOT targets to distinct relative targets.
2. Save two catalog entries with different sources and opposing explicit policies (Always versus Never). Both separate identities and both rules must remain.
3. Preserve ordinary drive-letter \\?\C:\... and UNC \\?\UNC\... spelling aliases to avoid redundant known apps.

The formatted, test-only [commit 0246ff5](https://github.com/TechWisard/desktop-pilot/commit/0246ff5fd345b79231b2bc4a1dcdc7c327655bbd) ran under standard [Windows CI 41275522698](https://github.com/TechWisard/desktop-pilot/actions/runs/41275522698). On **both Windows Server 2022 and 2025**, **353 preexisting tests passed and the same two new assertions failed**: the catalog contained **one** entry instead of **two**, and the namespace target was normalized to the same relative string. Formatting passed and compilation/test execution completed. An earlier unformatted candidate stopped at rustfmt and is not a runtime reproduction.

## Minimal correction and limits

Strip the prefix only for an ASCII drive-letter absolute path (\\?\X:\...). Preserve the full prefix for volume GUID, GLOBALROOT and other device namespaces. Continue existing extended UNC normalization. Keep Windows case-folding and URI payload identity unchanged. Do not rewrite existing saved files, counters, rules, historical ownership, or persisted IDs; there is no forced migration.

Acceptance: regressions pass in the real SQLite catalog; all existing suites for path spelling, URL identity, user choices, shortcut-file Apply/Undo remain successful; strict rustfmt, locked cargo tests, Clippy warnings denied, debug build, native integration and release guards on both Windows runners. No code-level collision test establishes success against a physically mounted/disconnected volume, recovery of previously overwritten old records, GUI usability at 200%, or real task-time improvements. Physical Windows 11 validation remains a follow-up.
