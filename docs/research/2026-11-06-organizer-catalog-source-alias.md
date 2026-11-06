# Saved catalog source aliases and explicit user control (2026-11-06)

## Source-confirmed risk

The existing desktop organizer performs the correct same-directory filesystem identity comparison for a number of Explorer view and launch-target aliases. But its Analyze and Apply guards previously compared saved **catalog source** paths against proposed Desktop moves by normalized strings alone (`plan.rs`). When a catalog record retains `Desktop\\.\\Alpha.url`, an 8.3 spelling, or an extended local spelling, that record may still name the Desktop file even though its text differs from the canonical enumerated path. This affects records with both explicit manual policies and Auto policy.

The persisted catalog source is rewritten by `journal.rs::update_catalog` using normalized path equality. It cannot necessarily be rewritten *after* a move when its alternate spelling has become non-resolvable. That creates two unwanted outcomes: a manual Always/Never source may be moved in spite of the explicit preference; an Auto source may be left referring to a path that no longer exists. This risk is established by source inspection and a new regression; it is **not** a reported production incident.

## Change and scope

The production `Dependencies` pass now identifies distinct non-canonical saved sources by verified local file ID **and** verified containing Personal Desktop directory ID. Such a source marks its corresponding physical Desktop entry ineligible for organization, including during the repeat dependency scan inside locked Apply. A catalog source in an unrelated parent directory does **not** protect a Desktop hard link merely because its file ID matches. Duplicate alias spellings are checked only once. Ordinary canonical catalog source paths and default shortcut-only analysis require no additional identity probes for those entries. No changes to SQLite schema, policy definitions, journal layout, filesystem rename/overwrite protections, privacy behavior, or recovery decisions.

A user can resolve the issue by changing the saved catalog source to its canonical Desktop file path and analyzing again. Preserving the file is preferable to silently changing an explicit rule or inventing an unverified path rewrite.

## Reproducible mixed sequence / evaluation

`saved_catalog_source_alias_and_manual_choice_survive_preview_apply_and_undo` in `src/organize/consent_tests.rs` exercises the actual Windows filesystem, native .url reader, `Store` and full production `analyze → apply → undo` paths:

1. Analyze three site shortcuts. Then change Alpha's saved catalog source to a same-folder dot-segment alias and set Always. Applying the old three-item plan must reject **before any folder creation or file move**, leaving policy and catalog unchanged.
2. A refreshed plan must leave Alpha alone but still propose the two independent sites, which can be organized and undone. This avoids making a single unusual catalog record disable all usefulness.
3. Releasing Always to Auto must not bypass the path-identity rule: a path still becomes stale after moving it, regardless of the policy setting.
4. Point the saved source at a real hard link in a **different** local directory. File-ID equality must not treat it as a Desktop source alias. Restoring the direct canonical Desktop source likewise permits the three-item operation; Apply and Undo must preserve each file and the stored catalog record.

Evaluation targets: 0 unwanted manual-source moves, 0 invalid saved source rewrites, 0 partial operations after stale previews, unchanged independent proposals, and no false hard-link exclusion. Test event counts are synthetic acceptance evidence, not real-user task timings, production reliability estimates, or proof of GUI behavior.

## External basis and evidence boundaries

* Microsoft Win32 `BY_HANDLE_FILE_INFORMATION`: compare volume serial and file index to establish file identity; file IDs are not permanent over deletion/recreation, so current open handles and path checks matter. https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information
* Microsoft Win32 `GetFileInformationByHandle`: compare volume serial plus file index for paths to the same file or directory. https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfileinformationbyhandle
* Findlater and McGrenere, CHI 2004, *A comparison of static, adaptive, and adaptable menus*: controlled laboratory findings favored user-controlled personalization over unpredictable automatic menus in several measures. This motivates respecting explicit manual choices, but it does **not** validate Desktop Pilot's particular algorithm. https://doi.org/10.1145/985692.985704
* Gajos et al., CHI 2008, *Predictability and accuracy in adaptive user interfaces*: improved accuracy and predictability raised subjective satisfaction in their study. This is general interface context, **not** evidence of customer behavior in this application. https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml

## Remaining acceptance

Real Windows 11 GUI and Explorer placement/Undo, RU/EN interface at 100–200% DPI, multiple monitors, redirected or OneDrive Desktop, interruption and restart recovery, and observation of task-level benefits remain independent release gates for draft #98. Unit/integration tests on Windows Server are not substitutes. Keep the organizer out of main/release until these pass.
