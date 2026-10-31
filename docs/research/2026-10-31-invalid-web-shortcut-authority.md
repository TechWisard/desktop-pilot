# Web shortcuts without a host should not occupy a quick slot — 2026-10-31

## Practical scenario and evidence boundary

A person imports a set of Internet Shortcuts. One saved `.url` retains `URL=https://` or an incomplete authority after a failed export, while another points to a working website. The malformed shortcut may carry a substantial historical score from an older profile. Under the existing `native::valid_target` implementation, every string starting with `https://` or `http://` was treated as valid, regardless of whether there was a host. This was directly confirmed by reading the production code. The resulting invalid entry could be ranked as eligible, occupying one of a small number of desktop quick slots.

Independent context, not evidence of this exact failure: users report different Windows Internet Shortcut breakages such as broken file associations ([Steam discussion, May 29, 2025](https://www.reddit.com/r/Steam/comments/1kyfzc6)) and project `.url` files failing to open after Windows/Office updates ([Office365 discussion, August 15, 2025](https://www.reddit.com/r/Office365/comments/1mqv0hu)). Neither report establishes how often malformed URLs appear in Desktop Pilot. The [CHI 2008 study by Gajos et al.](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml) associates adaptation accuracy with usability, while the [Findlater–Gajos 2009 review](https://www.eecs.harvard.edu/~kgajos/papers/2009/findlater09design.shtml) cautions about evaluation validity. These motivate preserving useful positions, not a claim of measured real-user time savings.

## Production-path regression

`native::tests::malformed_web_shortcuts_are_not_selected_even_with_old_usage_history` writes two actual `.url` fixtures, parses them with `native::read_link`, injects synthetic saved scores (broken 1000, working 20), and calls `engine::preview_loaded`, the same selection path used by the application. After the change the broken shortcut is ineligible and the usable one is selected for the single available slot. The test checks that the source file was not modified. The second test covers empty host, query-only, fragment-only, missing host after credentials, colon-only ports, whitespace and backslashes in the authority, while allowing a normal host, localhost with a port, a bracketed IPv6 literal, and spaces in a URL *path*.

The fix deliberately avoids URL canonicalization or network probing: no browser launch, DNS request, telemetry, or mutation of history, source shortcuts or manual rules. It rejects clearly unusable web authorities but does not claim to validate all possible URLs. Existing URI handling for Steam, shell and registered custom schemes is unchanged.

## Acceptance and scope

The source-level failure was established by inspection. Windows build, format, Clippy and native checks must pass in CI before merging. A CI pass does not demonstrate that a target website is reachable or guarantee behavior across all installed browsers. Interactive Windows 11/UI acceptance remains necessary for a release. This is one deterministic synthetic scenario, not a human time-saving measurement.

Tracked comparison for this fixture: before — malformed URL was marked valid and could take 1/1 slot with higher saved score; after — 0 malformed selections, 1/1 working shortcut selected, no source changes. No adaptation delay or manual action savings are inferred.
