# Search exclusion explanations in desktop organizer — 2026-11-08

## Observed gap, distinct from a user report

The unreleased organizer preview displays *why* each skipped item is left alone, including a manual shortcut rule, linked data, protected objects and incomplete dependencies. However, the source filter in `src/ui/organization.rs` matched skipped rows only by their filename. Searching the English explanation “manual shortcut” or its displayed Russian equivalent “ручное” returned no skipped entries when names did not contain those words. With many exclusions, a person could not review a safety category without paging through unrelated objects. This is a direct source observation, not a reported customer complaint.

Microsoft's [Windows Search AQS documentation](https://learn.microsoft.com/en-us/windows/win32/search/-search-3x-advancedquerysyntax) illustrates why search scope and exposed metadata matter, although this organizer does **not** implement AQS. Existing [review/predictability research](2026-10-26-adaptive-shortcuts.md) motivates consistency between the explanation shown and the information that can be retrieved. Reduced scanning effort is a hypothesis; no human timing study has been run.

## Change and acceptance

The existing search now covers each skipped entry's **already displayed, localized exclusion reason**, in addition to its name. The label says “Search by name, folder or exclusion reason” / “Поиск по имени, папке или причине исключения”. No extra controls, filter syntax, hidden actions, stored-schema changes, telemetry, automation or filesystem operations were added. Existing matching of normal suggestions by name and destination is unchanged.

The production fixture `searching_exclusion_reasons_finds_saved_manual_rules_across_pages_and_locales` creates 45 real temporary Windows `.url` shortcuts, imports them with `native::read_link` into SQLite, assigns explicit Never policies and adds an unrelated text file with another reason. The real analyzer returns 45 manual exclusions among 46 skipped objects and two movable suggestions. The real egui modal is rendered in English and Russian; a search by the displayed explanation must show a count of 45 in each language, while an unrelated query matches zero and filename search still works. No files are moved, no journal is created and existing selections remain intact. These are synthetic correctness results, not proven savings for real users.

## Gate

Run the same Windows 2022, 2025 and 11 ARM locked tests, strict Clippy, formatting, build and native organizer/Explorer/UI coverage on the exact proposed head. Keep the change on organizer draft integration until Windows 11 physical multi-monitor/DPI, real OneDrive, keyboard accessibility and interrupted-power checks are independently accepted. No 0.3.0 release from this change alone.
