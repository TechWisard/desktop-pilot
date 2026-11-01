# HTTP(S) host punctuation and one-slot recommendation safety — 2026-11-01

## Why this matters

Continue [the earlier authority and port checks](2026-11-01-web-shortcut-port-authority.md), rather than re-running the broader adaptive-interface research. The [WHATWG URL Standard, host parsing](https://url.spec.whatwg.org/#host-parsing) treats `<`, `>`, `^` and `|` as forbidden host code points. Microsoft documents [Internet Shortcuts](https://learn.microsoft.com/en-us/windows/win32/lwef/internet-shortcuts). The [Findlater/McGrenere CHI 2004 comparison](https://doi.org/10.1145/985692.985704) motivates caution with automatic rearrangement; it is **not** evidence of Desktop Pilot time savings.

**Confirmed by source inspection:** `native::web_target_has_host` rejected whitespace, control characters, backslash, malformed IPv6 authorities and invalid ports, but allowed these four forbidden host characters. Thus `native::read_link` marked a corresponding `.url` shortcut valid, enabling `engine::preview` to consider it for a scarce quick slot. This is a program-path defect hypothesis based on code and URL grammar, not a reported user incident.

## Narrow correction

Exclude precisely those four forbidden host characters in the authority's host/port portion, alongside the previously rejected characters. Preserve permitted punctuation in URL paths and userinfo, numeric ports, valid IPv6, and offline but syntactically valid destinations. Do not perform DNS, fetch links, open browsers, modify original files, rewrite scores, or change user settings. Leave non-HTTP registered protocols untouched.

## Reproducible production-path scenario

`native::tests::forbidden_http_hostname_characters_cannot_displace_usable_shortcut` creates two real inert `.url` files, parses them through `read_link`, saves both apps and retained seed scores into an actual `Store`/SQLite profile, and invokes the real `engine::preview` with one available slot. The malformed link has a historical score of 500, the valid link only 30.

**Before (source-derived expectation):** malformed `bad^host.invalid` is treated as available and may win the slot. **After (regression assertion):** the malformed link is invalid/ineligible; the valid documentation link is selected; the original fixture bytes are unchanged. Table-driven cases reject forbidden characters in hostnames, including after credentials, and preserve equivalent characters in paths and userinfo.

These are synthetic URLs and scores, not telemetry, user timings, launch attempts, or demonstrated productivity savings.

## Acceptance and remaining work

Run the unchanged Windows 2022/2025 pipeline: formatting, locked Rust tests, strict Clippy, build, native fixtures, release safeguards. Verify the exact branch SHA, then review and merge only after green results and no parallel conflicts. Interactive Windows 11, actual browser behavior, inaccessible redirected desktop folders, user-perceived recommendation stability and 200% scaling remain unverified. Do not publish a release before confirming the normal build and artifact gates.
