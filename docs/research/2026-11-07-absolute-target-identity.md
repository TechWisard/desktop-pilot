# Absolute launch paths with percent-shaped folder names — 2026-11-07

## Source evidence and boundaries

Microsoft documents that `IShellLink::GetPath` with `SLGP_RAWPATH` can return raw paths containing environment variables. This is not proof that every percent-shaped component in an *absolute* saved filesystem path is a variable. Desktop Pilot already distinguishes those cases when pasting a full path into Apps search, but did not apply the same distinction to saved launch identity, working-directory identity, and availability.

- Primary reference: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getpath
- Existing catalog search and literal-directory reproduction: `src/ui/catalog_executable_path_tests.rs` and `src/ui.rs`.
- This is a code-path defect with synthetic Windows filesystem fixtures, not a measured failure rate among users.

## Reproduced production-path contradiction

Given a real directory literally named `%USERNAME%` and a different directory named for the current Windows user, both containing `Inspect.exe`:

1. Before the change, `App::identity` expanded both absolute paths to the same normalized target. The resulting `assign_id` values collided, so a SQLite catalog could not reliably preserve independent Always/Never profiles.
2. Before the change, `native::valid_target` checked the expanded path for both. A valid literal file was reported unavailable until the other file existed, or a missing literal target could appear available because the expanded one existed.
3. An absolute working directory with a literal percent-shaped component similarly borrowed the state and identity of a different directory.

The change uses the *existing* literal path normalization for absolute Windows targets and working directories, while continuing to expand relative `%SystemRoot%\... ` launch templates. It does not modify stored user files, saved IDs, settings, database schema, existing manual choices, or desktop placement. Existing saved records are matched by their actual target strings on rescan rather than blindly merged by their formerly colliding generated identity.

## Regression and expected comparison

`src/model/literal_percent_tests.rs` uses real filesystem entries, `native::valid_target`, `App::identity`, `App::assign_id`, SQLite `Store`, `rank` and `select_at`. It checks:

- A literal target exists while the expanded target does not, then both exist.
- Independent saved Never/Always rules and reversal without loss of either catalog row.
- Same executable, distinct literal/expanded working directories.
- Relative Windows environment templates still resolve to the same logical target as their concrete path.
- Both underlying fixture files remain byte-for-byte unchanged.

Before: **1 identity for 2 distinct targets**, and a false-negative availability result for the literal file. Expected after: **2 distinct identities** and correct availability, with no extra user action. These are synthetic correctness checks, not time-saved measurements.

## Acceptance

Keep all current Windows checks, native filesystem and SQLite integration, strict Clippy and release guards. Require Windows CI evidence before merging. A hosted test is not interactive Windows 11 UX, OneDrive, multi-monitor or interrupted-operation acceptance. The unrelated draft organizer remains unreleased.
