# Preserve released 0.2.129 in the unreleased organizer branch — 2026-11-09

## Confirmed starting points

- Current published stable source/release commit: `58fca6e1821292053b0bd801bced1295f0fc4523` (v0.2.129).
- Organizer integration after the independently tested local-lyrics safeguard (#259): `0a4937431c822afa1b383f75d5d86a67ea137113` (still unreleased 0.3.0).
- The integration branch had diverged by three stable commits since v0.2.128. A final organizer release without reconciling the ancestry could obscure the provenance of the already-shipped sustained-focus correction.
- The production `src/model.rs` blob is **byte-for-byte identical** on the two starting branches (c9cf533bdb1e31edf50677ab4ddc45eef8bfe30f); no second interpretation of the weighting formula is introduced.
- The similarly named `src/model/cold_start_tests.rs` tests already exist in both branches. Their relevant differing call sites are deliberate: integration has an additional preflight callback path for organizer coordination, while stable calls the original review method. Do not overwrite that integration test code with the stable copy.

## Non-destructive reconciliation

Use a two-parent merge commit with the integration head as the first parent and v0.2.129 as the second. Preserve the integration tree, its 0.3.0 Cargo version, all organizer tests, worker serialization and native Windows checks; add both new stable release documents without replacing organizer behavior. Correct the English and Russian README release banners so visitors do not mistake the current 0.2.129 download for the draft organizer. Include a short linked description of the sustained-work fix.

No source file, executable, collection permission, manual policy, persisted usage or user data is modified by this merge. This is history/doc continuity, **not** a new product capability or a version release.

## Validation boundary

The combined two-parent tree must pass exact-head Windows 2022/2025/Windows 11 ARM `cargo fmt --check`, locked Rust tests, strict Clippy, build, native Explorer, Cloud Files and UI scenarios before it can become the integration head. The independent stable v0.2.129 release checks do not replace that merge-tree test. Do not modify stable `main`, publish 0.3.0 or waive physical Windows 11 multi-monitor/DPI, real OneDrive-account, keyboard/accessibility and power-interruption acceptance. Public synthetic tests measure correctness, not user time saving.
