# Exact pasted web addresses in the shortcut catalog — 2026-11-05

## Observed source behavior

The production `CatalogCache::update` in `src/ui.rs` already matches a complete copied `.lnk`, `.url` or `.exe` filesystem path against actual shortcut identities. It did not handle a complete HTTP(S) address the same way: that input fell through to tokenized substring matching over the combined app name, destination, launch arguments and user note. An unrelated saved shortcut whose note mentions the URL, or a different address that merely starts with it, could appear when the user searched for the exact saved website. This is a source-confirmed issue, not a report of an observed Desktop Pilot user incident.

## Independent context and hypothesis

- Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004): controlled evidence that user control and stable item selection matter for personalization: https://doi.org/10.1145/985692.985704
- Findlater et al., *Ephemeral Adaptation* (CHI 2009): dynamic guidance can preserve spatial consistency: https://www.cs.ubc.ca/labs/imager/tr/2009/findlater_chi_ephemeral/
- Microsoft Edge Favorites documentation confirms a user's saved website has an address separate from its display label: https://support.microsoft.com/en-us/microsoft-edge/add-a-site-to-my-favorites-in-microsoft-edge-eb40d818-fd1f-cb19-d943-6fcfd1d9a935
- An independent Edge user describes frustration that search results omit context about a favorite's location (May 23, 2025): https://www.reddit.com/r/MicrosoftEdge/comments/1ktkzlx/ . This is about Edge, **not** Desktop Pilot, and does not establish the frequency or severity of the Desktop Pilot issue.

**Design hypothesis:** Users pasting a full address to locate a particular shortcut expect it to identify that destination rather than a note or URL prefix. By contrast, searching just a domain fragment should remain broad.

## Change and production-path reproduction

The search cache now treats a complete non-whitespace `http://` or `https://` address with a nonempty authority as an exact target query. Both query and app use the existing `model::target_identity`, which normalizes the HTTP(S) scheme and host while preserving path, query and percent-encoded content case. The search input may have a matched pair of surrounding single or double quotes; all other text search is unchanged. Existing manual-policy filters and explicit sorting remain in control. No files, launch profiles, SQLite data, privacy behavior or background collection are modified by a catalog search.

The regression `pasted_web_address_selects_its_real_shortcut_not_notes_or_longer_links` runs the **production** `CatalogCache` against four apps persisted in a real `Store::open` SQLite catalog, using `rank`. It checks exact address identification (including copied quotes and host case), a note decoy, a longer URL, a distinct case-sensitive path, Never/Always filters, broad domain search and read-only persistence.

**Before:** a full address can return the desired app plus the decoy and the longer address. **After (expected):** only the actual matching target; partial domain searches remain broad, including the three targets and the note reference. These are synthetic matching counts, not observed time savings for people.

## Acceptance boundaries

Run the unmodified strict Windows CI, including Rust tests, formatting, Clippy with warnings denied, build, native integration and publication guards. This is not a substitute for manual Windows 11 GUI checks with URL paste, long paths, and 100–200% scaling. The separate draft desktop organizer (PR #98) retains its independent filesystem Undo, redirected desktop, interruption and visual acceptance gates.
