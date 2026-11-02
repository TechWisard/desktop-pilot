# Empty protocol shortcuts must not displace usable choices — 2026-11-02

## Grounded observation and user impact

Continue the existing [web-authority regression](2026-10-31-invalid-web-shortcut-authority.md), [invalid protocol handling](2026-11-01-web-shortcut-port-authority.md), and [adaptive-shortcut research](2026-10-26-adaptive-shortcuts.md) without repeating their full literature searches. Microsoft examples demonstrate real launch targets such as `shell:AppsFolder` ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/9915638f-f6a4-4b6e-89f4-ddbee5b3ff35/can-you-shine-a-light-into-my-matrix?forum=windows-all)). Valve's [Steam browser protocol](https://developer.valvesoftware.com/wiki/Steam_browser_protocol) specifies commands such as `steam://rungameid/<appid>`; Valve's [public issue #12264](https://github.com/ValveSoftware/steam-for-linux/issues/12264) shows an actual `steam://run/<appid>` use case. These sources **do not** measure Desktop Pilot usage or prove that arbitrary protocol URIs always launch successfully.

**Confirmed source-path defect:** `native::valid_target` previously returned true for any target beginning `steam://`, `shell:`, or `::{`, even if the target consisted only of that incomplete prefix. A malformed imported or old saved `.url` could therefore acquire `valid = true` and, with a large persisted score, be selected over a usable shortcut. This conclusion follows from the existing production parser/selector; no real-world incidence is claimed.

## Narrow change

Reject empty Steam command payloads, empty Shell folder payloads and incomplete Shell namespace prefixes in the existing validity check. Keep nonempty Steam command targets, `shell:AppsFolder`, and complete Shell GUID namespace paths accepted as before. Do not probe networks, execute shortcuts, read private activity data, change rankings, add settings or touch user originals. This is deliberately conservative validation of obviously incomplete targets, **not** a full authority parser for all URI schemes.

## Reproduction

`incomplete_protocol_shortcuts_cannot_displace_a_working_choice` writes real Windows `.url` fixtures, reads them with `native::read_link`, assigns a high saved historical score to each incomplete candidate, and invokes the exact `engine::preview_loaded` selector with one available slot. Before the change these empty targets passed validation; the regression requires the working web shortcut to remain selected and the original files to remain byte-identical. Valid Steam, Shell and namespace examples are explicit regression controls. All evidence here is synthetic and does not claim measured human time savings.

## Acceptance boundaries

Run unchanged Windows Server 2022/2025 format, Rust unit and integration tests, strict Clippy, build, native fixtures and release guards. Physical Windows 11 shell execution, program registration, Explorer UI at 100–200%, and network/unavailable-volume behavior remain unverified. Do not publish this change as a standalone release before verified checks and a separate release/package consistency review.
