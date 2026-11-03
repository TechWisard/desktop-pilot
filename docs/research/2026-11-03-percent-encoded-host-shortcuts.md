# Percent-encoded HTTP hosts and incorrect quick-shortcut recommendations — 2026-11-03

## Scenario and independently established behavior

Continue the previously documented [web authority](2026-10-31-invalid-web-shortcut-authority.md), [HTTP port](2026-11-01-web-shortcut-port-authority.md) and [host punctuation](2026-11-01-http-host-punctuation.md) investigations; do not repeat the general adaptive-menu literature search. A saved Internet Shortcut may have a URL like `https://work%2Fproject.invalid/dashboard`. The percent-encoded slash occurs **in the hostname**, not the URL path. The [WHATWG URL Standard](https://url.spec.whatwg.org/#host-parsing) specifies percent-decoding for HTTP(S) host parsing and prohibits decoded slashes, at-signs and controls in domain names. Microsoft's [Internet Shortcuts documentation](https://learn.microsoft.com/en-us/windows/win32/lwef/internet-shortcuts) describes how a `.url` shortcut is opened by the registered handler, usually the browser.

These are primary specifications and code-path facts; they do not establish that a Desktop Pilot user actually imported this malformed URL or measure how often it occurs. The user-facing hypothesis is that a malformed, previously popular shortcut could displace a genuinely usable one in a small quick-shortcut selection.

## Confirmed source-level failure and red regression

`native::read_link` calls `valid_target` and `web_target_has_host`. The old authority check rejected literal forbidden characters and malformed ports but never decoded/checks percent-encoded bytes in a hostname. `https://work%2Fproject.invalid/dashboard` was therefore stored as `valid=true` even though the decoded host contains a slash.

The **test-only** regression `native::tests::percent_encoded_forbidden_http_host_bytes_cannot_take_a_quick_slot` creates real local `.url` files, parses them using the production Windows Internet Shortcut reader, retains synthetic scores of 500 (invalid) and 30 (valid), saves the actual app catalog and config in SQLite, then uses `engine::preview` with one available slot. It also checks that original bytes are unchanged and directly checks encoded delimiter/control cases plus permitted encodings elsewhere in the URL. [Windows Server 2022 run 41608216510](https://github.com/TechWisard/desktop-pilot/actions/runs/41608216510) reproduced the failure **before any production fix**: rustfmt and publication guards passed; 401 existing Rust tests passed, the new test failed at the expected `bad.valid` assertion. A previous formatting-only failure (run 41608035950) does **not** count as behavior evidence.

## Minimal correction and scope

Reject malformed percent escapes or percent-encoded host bytes that decode to forbidden HTTP domain characters, while preserving normal encoded hostname letters, permitted userinfo, path/query encodings, IPv6 validation and the existing port check. The guard is local, bound to the HTTP(S) host/port authority after userinfo separation. There is no DNS, browser launch, network traffic, catalog rewrite, change to manual rules, changed GUI, telemetry or new configuration. Correct valid-source recommendations and original source files remain preserved.

**Before (Windows regression):** the broken high-score target was considered valid; an inaccurate recommendation could take the only slot. **After (target assertions):** broken target is invalid/ineligible; valid target takes the one slot and source bytes stay unchanged. These are synthetic selection results, not demonstrated human task-time savings or a guarantee that all apparently valid URLs resolve.

## Validation and remaining acceptance

Run the unchanged public Windows Server 2022/2025 workflow on the corrected branch: strict format, all Rust tests, warnings-denied Clippy, build, native integration and release guards, and repeat on the PR / merged commit. Record exact run results; do not call a test-only red run a green implementation test. Physical Windows 11 Explorer behavior, actual browsing, real malformed URL incidence, long-term habit changes, UI scaling, usability and accessibility require separate acceptance. Keep draft organizer PRs #98 and #101 separate.
