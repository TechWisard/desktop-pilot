# Opening an established profile during another writer — 2026-10-27

## Context, evidence and inference

Baseline is main `ee9d41e0844cccf535e40424c3457f89c3959ae4` (version 0.2.24). Continue [cold-start issue #22](https://github.com/TechWisard/desktop-pilot/issues/22), [catalog snapshot](2026-10-27-catalog-snapshot.md), and [no-op rescans](2026-10-27-noop-catalog-rescan.md); do not repeat the full previous HCI survey. An independent [Flow Launcher issue #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), reported May 21, 2026, concerns large custom catalogs unavailable after restart in another program. It motivates a test, not proof of this defect or of real user time lost in Desktop Pilot.

The primary [SQLite WAL documentation](https://www.sqlite.org/wal.html) describes concurrent readers and a single writer. The [SQLite BEGIN IMMEDIATE documentation](https://www.sqlite.org/lang_transaction.html) specifies that a second write transaction can fail with SQLITE_BUSY if a writer is already active. The actual Desktop Pilot `Store::open` preflights the schema read-only but unconditionally calls `initialize_schema`, which begins an IMMEDIATE writer transaction, even when the profile's tables were already complete and verified. This is a source-confirmed unnecessary lock, not a measured end-user startup delay.

## Actual-code regression

`src/store/open_writer_tests.rs` creates a complete real WAL profile with its actual `Store`, persists settings, starts a separate uncommitted writer transaction and calls the real `Store::open` before releasing the writer. Opening should succeed with committed settings visible, uncommitted events invisible. Against the unchanged production startup, the second `BEGIN IMMEDIATE` blocks until the five-second SQLite busy timeout and fails. In the corrected version, the verified profile opens without a schema writer transaction.

The existing concurrency, unrelated database, interrupted first creation and nested transaction tests continue to protect the empty/new database path. The fix changes no data model, ranking rules, UI controls or shortcut behavior. Before/after is successful open versus SQLite busy failure under the same deliberately held writer; real Windows 11 task-time savings and prolonged network folder delays are **not measured**.

## Safety boundary

Perform read-only schema verification before opening writable. If the schema is complete, revalidate it on the exact connection and skip table creation. Reject an existing file that becomes empty/foreign between preflight and opening, without initializing it. Fresh and verified-empty databases continue through the original immediate transaction and atomic table creation, including concurrent first-starts.

## Verification and continuation

Run unchanged Windows CI (`cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integration and publication guards) on two existing runners, check failure-first evidence, and verify tag/assets if published. A deterministic overlapping-writer test is not a claim about typical writer duration. Remaining issue #22: live Explorer/Windows 11 timing, 200% DPI interaction, inaccessible cloud/network directories, user-action latency under large worker scans, and multiday changing-project recommendations.
