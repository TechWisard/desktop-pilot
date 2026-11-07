# Exact web root lookup without changing saved launcher identities — 2026-11-07

## Evidence and scope

RFC 9110, section 4.2.3, specifies that an empty HTTP(S) path and "/" identify the same resource outside OPTIONS request-target handling: https://httpwg.org/specs/rfc9110#section-4.2.3 .

Desktop Pilot already treats copied complete HTTP(S) URLs as exact catalog searches rather than searching unrelated notes. However, its catalog compares strings produced by `target_identity`, which deliberately preserves stored URL path spelling. Thus an existing `.url` launcher for `https://example.invalid?view=Work` is not found when the user copies `https://example.invalid/?view=Work`, despite the equivalent HTTP root address. The converse occurs for a saved slash form and a copied slashless URL. This is a source-confirmed correctness gap, not a reported failure rate.

The correction is **search-only**: canonicalize the root slash during exact catalog lookup after the existing scheme/host-case handling. Do not change `App::identity`, `assign_id`, stored target strings, policies, profiles, launches, source files or on-disk history. In particular, an unrelated note mentioning the URL is not a match; HTTP and HTTPS remain distinct, paths and query values remain case-sensitive. No remote requests or telemetry are introduced.

## Production-path regression

`copied_web_root_urls_find_saved_profiles_without_broad_note_matches` writes four synthetic local `.url` files, loads them through `native::read_link`, saves and reloads real SQLite `Store` entries with distinct Always/Never/Auto rules, and runs the actual `CatalogCache::update` search/filter path.

Cases:
- Root URL with and without "/" before a query and before a fragment.
- Different host, scheme, path and case-sensitive query values.
- Unrelated saved note mentioning the copied URL.
- Always/Never filters, unchanged source bytes and unchanged number of catalog rows.

Before: an exact root URL spelling variant yields **0** matching profiles in the constructed case. After: it finds the intended saved profile, without extra search steps or broad false positives. This is a deterministic synthetic correctness comparison, not a measured improvement in human task time.

## Acceptance and boundaries

Require unchanged public Windows CI: formatting, locked tests, strict Clippy, locked build, native integration and publication guards. Hosted checks do not establish physical Windows 11 UI interaction, multi-monitor behavior, OneDrive, power interruption or the separate organizer release acceptance. Do not change release version or publish until the complete acceptance decision.
