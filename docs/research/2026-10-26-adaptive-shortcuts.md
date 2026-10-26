# Adaptive-shortcut discovery and profile search — 2026-10-26

## Evidence and hypotheses

- Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus*, CHI 2004: in their 27-person study, the adaptable menu was preferred by most participants; spatially shifting adaptive menus were not consistently faster. https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/
- Gajos et al., *Predictability and Accuracy in Adaptive User Interfaces*, CHI 2008: measured improvements in both accuracy and predictability improved satisfaction; accuracy also improved use and performance. https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml
- Findlater et al., *Ephemeral Adaptation*, CHI 2009: a non-rearranging adaptive approach improved visual-search performance while preserving spatial consistency. https://www.cs.ubc.ca/labs/imager/tr/2009/findlater_chi_ephemeral/
- User-reported desktop layout instability after resolution changes illustrates the cost of surprising movement; this is anecdotal evidence, not a measurement of Desktop Pilot. https://www.reddit.com/r/windows/comments/1arx8hj/

These references motivate keeping user-authored rules, stable placement, and predictive accuracy more important than adding widgets or causing frequent changes. No external user study of Desktop Pilot has been performed.

## Confirmed source inconsistency

`native::valid_target` accepts Windows `shell:` launch targets, Shell namespace IDs, and registered URI handlers. During first catalog discovery, `engine::refresh_apps` used to assign `Policy::Never` to targets lacking a physical executable or `://`. This included otherwise valid `shell:AppsFolder\\Package!App` targets, so `observe_app_id` could recognize actual engagement, yet the ranking had been forcibly disabled. The fix restricts the conservative exclusion to **invalid** non-file targets. Validity is still checked using the existing native logic; imported manually excluded entries remain excluded.

The Apps catalog search previously omitted `App.arguments`, `App.cwd` and `App.note`. Two shortcuts with the same executable and name but different launch profiles could not be found by profile name. The catalog now indexes these local fields, without changing the app identity or sending any data elsewhere.

## Reproducible regression scenarios

1. Fresh desktop with a `shell:AppsFolder\\Publisher.App!Main` shortcut: discover as valid `Auto` rather than `Never`; correlate the Windows App ID; record verified foreground usage; preview, apply, and verify the original bytes are untouched.
2. Explicitly set the same app to `Never`, rescan, and confirm the rule persists and it is removed from automatic selection.
3. Search two same-named shortcuts with distinct `--profile-directory` arguments; searches by profile, working-directory component and user note must resolve to the correct catalog entry.
4. Retain all existing native integration tests for interrupted undo, replaced files, conflicting profiles and screen geometry.

## Verification boundaries

These are in-process regression fixtures exercising the real catalog, ranking, desktop-copy and Settings search paths on Windows. They do not demonstrate successful interactive launching of every third-party URI or measure the time saved by actual users. Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, and `scripts/test-native.ps1` in Windows CI before merging.

## Next scenarios

- Validate Shell namespace and packaged-app links with real Windows installations, not only synthetic shortcut files.
- Study mixed browser-profile workflows where the process executable is ambiguous; never attribute one profile's usage to another without specific launch identity.
- Quantify false-positive placements, unnecessary replacement count, time to first actionable recommendation and stability across user-project changes.
- Continue manual-rule, uninstall/reinstall, desktop-topology and recovery simulations while protecting existing user files.
