# Windows extended UNC spelling and stable shortcut identity — 2026-10-29

## Starting state and evidence

Main at inspection: public Desktop Pilot 0.2.45, commit `15af9b49ae1340bcf62582549f113c5e08aa5804`, with successful normal Windows Server 2022/2025 CI and no open PRs. The separate `TechWisard/finite-checks` repository was not found and is unnecessary; keep standard public CI. Prior source/research on network outages, profile identity, and protected manual choices is retained in `docs/research/2026-10-26-adaptive-shortcuts.md` and issue #22.

Microsoft's primary Windows file-naming documentation (https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) describes ordinary UNC network paths and their extended-length `\\?\UNC\` spelling; case-insensitive path semantics are the Windows default. This is reference context, not evidence that a particular user has encountered the defect.

## Confirmed source boundary

`model::norm` already mapped the uppercase `\\?\UNC\Server\Share\...` prefix to the ordinary UNC spelling, but the prefix check used Rust's case-sensitive `strip_prefix`. A lowercase or mixed-case `unc` device component instead fell through to removal of only `\\?\`, producing `unc\server\share\...` rather than `\\server\share\...`. The same Windows shortcut target could therefore obtain a different `App::assign_id` and lose stable catalog identity when a path representation changed. The test checks an identity mismatch, not a verified real-network outage.

The regression uses the **production** `norm`, `target_identity`, and `App::assign_id` implementations for ordinary, uppercase, lowercase and mixed-case extended UNC spellings. It also protects local-drive versus remote-target separation and case-sensitive HTTP URL path identity. No independent model substitutes the actual identity logic.

## Correction and constraints

Check only the eight-byte ASCII namespace prefix case-insensitively. Use `str::get` before slicing, so an unrelated Unicode path cannot panic at a non-character boundary. Preserve all remainder characters and the existing rules for ordinary drive paths, case-sensitive URL components, arguments and working directories. No filesystem probing, user data, schema, dependencies, settings, background traffic, telemetry, automatic placement or change to source files.

The initial new-test branch run `40818961045` stopped at Rust formatting and is not behavioral evidence. The formatted baseline is `6797badd723c02c7dd58c907003daafa124906c5`, with unchanged production normalization; its independent Actions run is https://github.com/TechWisard/desktop-pilot/actions/runs/40819101785. Inspect the actual test failure before calling the defect reproduced. The corrected branch and PR/main CI must each complete ordinary `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, 28 native integration fixtures, README and nine publication guards. Release only after full standard packaging and tag checks.

## Limits and continuation

This fixture verifies lexical Windows path identity in the real application code; it does **not** connect to a real SMB server, confirm the availability of a path on an offline share, measure cold-start/UI latency, or prove time saved. All previously saved manual choices and file-safe Undo must remain intact in the full existing integration suite. Physical Windows 11/200% DPI, real network disconnect/reconnect, offline profile disks, and longitudinal project changes remain in issue #22.
