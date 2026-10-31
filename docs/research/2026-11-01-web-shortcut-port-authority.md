# Invalid web ports must not displace working quick shortcuts — 2026-11-01

## Baseline and practical scenario

Started from published **v0.2.77**, main commit `dd88373082e719ab098b968f260678b632740eca`. The prior URL-authority correction rejected empty hosts, but the production `native::web_target_has_host` still accepted explicit nonnumeric and out-of-range ports, or malformed IPv6 brackets. An imported `.url` whose address is `https://example.invalid:word/project` would therefore be considered launchable. Given higher retained historical usage, this invalid shortcut could take a scarce quick slot from an otherwise valid link. This is a source-identified and synthetically reproduced failure, **not** an observed customer incident or measured productivity impact.

The [WHATWG URL Standard](https://url.spec.whatwg.org/) identifies a nonnumeric HTTP(S) port as invalid (`port-invalid`), a port outside the allowed range as `port-out-of-range`, and unclosed/malformed bracketed IPv6 as a validation failure. This is a syntax check only; it must not attempt DNS, network requests, or otherwise flag a temporarily offline but structurally valid website. [Microsoft Internet Shortcuts](https://learn.microsoft.com/en-us/windows/win32/lwef/internet-shortcuts) documents the relevant Windows shortcut mechanism. [Findlater and McGrenere, CHI 2004](https://doi.org/10.1145/985692.985704) and [Gajos et al., CHI 2008](https://www.microsoft.com/en-us/research/publication/predictability-and-accuracy-in-adaptive-user-interfaces/) provide existing research context for predictable, controlled adaptations, **not** measured Desktop Pilot time savings.

## Failure-first production-path reproduction

The added `native::tests::malformed_http_ports_cannot_occupy_quick_slots` test creates two actual `.url` files. It reads them through the native parser and submits their real `App` values to `engine::preview_loaded`, with one permitted quick slot and a substantially higher persisted-style seed for the invalid link. It also exercises the target guard for multiple port and IPv6 variants, including valid numeric ports, `localhost`, valid bracketed IPv6 and credentials. It verifies both source file byte sequences are unchanged. These are inert local fixture files: no browser or network call is made.

Test-only commit [b84361a](https://github.com/TechWisard/desktop-pilot/commit/b84361a0836f6f94d4e7eb1c9d46b52e3c06bb73) reproduced the same failure on **both Windows Server 2022 and 2025** in [workflow run 41242024233](https://github.com/TechWisard/desktop-pilot/actions/runs/41242024233): formatting passed, **346 Rust tests passed and the new regression failed** on each runner with `a nonnumeric HTTP port is not launchable`. The initial test authoring commit `64939c8` failed formatting before testing and is **not** counted as behavioral evidence.

## Narrow correction

The parser now separates an optional explicit port from ordinary hostnames or valid bracketed IPv6, rejects nondecimal or out-of-range ports and unmatched IPv6 brackets, and keeps accepting an empty explicit port as browsers do. It preserves existing path/query whitespace handling, protocol schemes other than HTTP(S), historical records, user exclusions, and local-only behavior. It adds no new settings, network lookups, telemetry, file mutations or background work.

**Before (observed in the real test path):** invalid higher-priority link was marked valid; the intended one-slot recommendation was not safe. **After (acceptance target):** zero invalid placements, valid shortcut selected, unchanged original files. The before/after measure is a deterministic synthetic recommendation, **not** real-user time saved.

## Acceptance and follow-up

Verify the branch with the unchanged Windows 2022/2025 workflow: rustfmt, all locked Rust regressions, warnings-denied Clippy, locked build, native integration and release safety guards. Confirm any merge/release against exact commit/tag and published executable/assets. Related draft desktop-organization work (PRs #98, #101) stays untouched.

Unverified by this scenario: installed physical Windows 11 GUI, actual browser navigation, redirected or offline desktop roots, longer project/habit shifts and human time-to-first-benefit.
