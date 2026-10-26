# Manual absence: completion and handover (2026-10-26)

Continues [the original investigation](2026-10-26-manual-absence.md), not a new ranking policy. The published baseline was 0.2.9; PR #8 already held the manual-retention correction and two failure-first regressions. Existing profile recovery, bounded SQLite retries and user-control safeguards were retained.

## Additional confirmed defect and correction

The new protected-offline warning and the ordinary grace warning were English-only. Adding translation fragments alone would not fix the old sentence structure: the runtime translator deliberately protects everything following a Windows/UNC path until the next line. Put the complete explanation before the path and translate that prefix. No path parser or broad replacement of filenames is introduced. Two tests run the real translator both ways for local paths, UNC paths, Cyrillic folders and filenames containing words that are themselves status phrases.

## Reproduced sequences and measured scope

The production Store/refresh/preview/apply regression now checks six successive reviews at simulated source-outage ages **1, 2, 3, 6, 21 and 45 days**, followed by restoring the original and explicitly switching Always to Auto. Every outage review preserves one protected shortcut, its exact bytes, path, filesystem identity and position. Selection-level tests cover the ordinary automatic expiry, Never, legacy manual placement and changed launch identity. The original pre-fix six-day reproduction is preserved in the earlier document. Zero unsolicited replacements in the protected sequence is a synthetic result, not measured time saved by users.

## Executed checks

Preparation run: https://github.com/TechWisard/desktop-pilot/actions/runs/40354518771. Immutable input: `d0fb8578fe377b749319ea8d4702b933210dd867`. The two new localization tests first failed on the old translator (0 passed, 2 failed). After correction, **174 Rust tests passed, 0 failed, 0 ignored**; formatting, warnings-denied Clippy, debug build, **28 native integration checks** and **9 publication guards** passed on Windows 2022 with Rust 1.93.1. The UI layout/interaction tests included in the full suite ran; exact test names and outcomes are in [the machine-readable record](2026-10-26-manual-absence-validation.json).

The preparation workflow creates detached Git objects only. Its maintenance workflow/script are not part of the candidate source tree. Main history, branch protection and repository visibility are not changed by preparation. Branch advancement uses an expected-head check. The normal unchanged Windows 2022/2025 workflow must still pass for the candidate and main before its existing release job can publish. Final PR/run/tag/asset verification is recorded in PR #8; the preparation pass alone is not a release certificate.

## Evidence reused and extended

- Findlater and McGrenere, CHI 2004, compared static/adaptive/adaptable menus with 27 participants: https://doi.org/10.1145/985692.985704.
- Findlater et al., CHI 2009, report visual-search improvements while preserving spatial consistency: https://www.cs.ubc.ca/labs/imager/tr/2009/findlater_chi_ephemeral/.
- PowerToys user report, 2025-04-16, describes unpredictable layout restoration when switching between home and office: https://github.com/microsoft/PowerToys/issues/38900. This is analogous user evidence, not the same root cause or an incidence estimate for Desktop Pilot.

The choice to preserve manually protected copies through a longer outage is an engineering interpretation of user control and continuity, not a result established by those studies for this product.

## Remaining work

Physical Windows 11 Explorer, real unplug/reconnect, visual reading at 200% in an interactive session and human usability/timing were not tested. Complete loss of the whole custom profile drive, broader long-term project changes and adaptation delay remain separate investigations. No new services, telemetry, schema migration, permissions request or user-facing settings were added.
