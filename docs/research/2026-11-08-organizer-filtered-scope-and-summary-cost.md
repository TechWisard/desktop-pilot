# Reveal hidden selected objects and avoid preview cloning — 2026-11-08

## Confirmed behavior and a real review scenario

A user analyzes two actual Windows Internet Shortcut files (Alpha.url and Beta.url), both initially selected. Searching for `Alpha` displays one row but the existing fixed `Apply selected moves` command will still apply the complete plan of **two moves**. The scrolling content warns in general terms, but the fixed confirmation header does not show that a selected object is outside the filter. This discrepancy is confirmed directly in `Settings::organization_dialog`, `organizer_matches_query`, `Plan::summary` and the existing Windows/SQLite/egui fixture. This is a risk of *misunderstanding the scope*, not an observed data-loss event.

A second source-level performance finding: `Plan::summary()` calls `movements()`, which clones every eligible `Item` (name, stamp, launch data) into a new vector. The organizer computes a summary for both the fixed header and scrolling body on every painted frame. A maximum-size 6000-item review therefore allocates and clones up to thousands of entries more than needed. The actual file-move path retains `movements()`; only the read-only summary is optimized.

These are product-source observations. [NN/g, *Preventing User Errors* (2015)](https://www.nngroup.com/articles/user-mistakes/) advocates showing the scope of an action and supporting an understandable preview; [NN/g, *Confirmation Dialogs Can Prevent User Errors — If Not Overused* (2018, reviewed 2026)](https://www.nngroup.com/articles/confirmation-dialog/) cautions against extra repetitive confirmations. They motivate a compact, visible count instead of another approval step. Neither source demonstrates that Desktop Pilot users have made this error or quantifies any saved time.

## Change and measurable synthetic effect

When a filtered review contains checked objects not matching the query, the **fixed header** now says `Selected outside search: N` / `Отмечено вне поиска: N`. It disappears when the hidden selection is unchecked or the filter is cleared. The original single Apply / Remember button, whole-plan semantics, per-item choices, folders, history and Undo are unchanged. No extra query scan is performed without an active search. The label counts checked **rows**, not guaranteed moves: selected singleton folders intentionally remain in place.

On the real two-shortcut case, fixed-header visibility changes from **no indication** to **one hidden checked object**; deselection changes the count to zero without moving either file. Counts update in RU/EN at 1x/2x. These are synthetic egui/SQLite correctness checks, not a measured reduction in user mistakes.

The optimized `Plan::summary` scans selected items once and aggregates by group. It no longer produces cloned `Movement` entries. The actual `Plan::movements` path is untouched, and a new 6000-item test compares both results across eight changing selection patterns including reused folders, created singleton folders, missing folders and fully deselected plans. Its assertion is semantic equivalence, **not** a throughput benchmark; do not advertise an unmeasured speedup.

## Release and acceptance boundaries

Run all existing locked Rust tests, strict formatting and Clippy, native Explorer/Cloud Files checks and RU/EN 100%-200% screenshots on the exact proposed head. The organizer is still the unreleased 0.3.0 draft; physical multi-monitor Windows 11/DPI, a live OneDrive account, interactive keyboard and actual power interruption remain open. The stable 0.2.128 product is not modified by this work.
