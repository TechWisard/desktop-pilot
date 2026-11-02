# Manual policy save during a concurrent catalog edit — 2026-11-02

## Evidence and the observed race

Continue the existing concurrent-catalog and manual-control research; no general interface literature survey is repeated. SQLite's own [transaction documentation](https://www.sqlite.org/lang_transaction.html) and [WAL isolation explanation](https://www.sqlite.org/isolation.html) establish that one connection can read an older committed app row while another connection has an uncommitted edit, and that `BEGIN IMMEDIATE` reserves the writer before reading.

The source at `12011d73e2ca5388a11efce844b5a16688608e40` used `Store::policy` as two separate autocommit operations: `app(id)` then `put_app`. A simultaneous catalog/import update on a second SQLite connection can occur between these operations. The policy writer can then replace the entire JSON record with its stale version, losing a newly edited display name or user note. This is a verified non-atomic code path, **not** a measured frequency of real-user data loss. The catalog scanner already merges concurrent policy/metadata under its own transaction, making the separate policy path an avoidable inconsistency.

HCI context remains the established [CHI 2004 adaptable-menu comparison](https://doi.org/10.1145/985692.985704) and [CHI 2008 predictability study](https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml): explicit user choices and predictable behavior matter; neither study measures Desktop Pilot's performance.

## Production-code correction and regression

`Store::policy` now uses the existing `Store::transaction` writer-first API for the *entire* read–modify–write operation, with no schema changes and no new runtime state. Existing nested transaction callers use the already supported savepoint mechanism.

`store::tests::manual_policy_does_not_overwrite_concurrent_catalog_metadata` uses two actual Desktop Pilot `Store` connections to the same SQLite WAL database. One connection holds `BEGIN IMMEDIATE` and updates a shortcut's name/note; another attempts to set `Always`. A test-only, thread-local observation point reports when the production policy path has read its row (absent from non-test builds). The regression ensures that this read **cannot** happen before the first writer commits; it then checks the stored `Always` choice, both newly edited fields, and a later `Auto` reversal preserving those fields. The observation uses a bounded 350 ms wait, not a runtime delay.

**Before:** the separate read was allowed while the other writer was active, risking a stale full-row replacement. **After:** the policy read must wait for the writer, then update its freshly committed record. One contested policy request causes no extra catalog rescan or file move. This is a correctness guarantee about the constructed SQLite interleaving, not a claim about measured user time.

## Required verification and limits

Run unchanged Windows Server 2022/2025 `cargo fmt --check`, `cargo test --locked`, strict `cargo clippy --locked --all-targets -- -D warnings`, build, native integration, README and release guards. Preserve original shortcuts, local storage, manual placement, existing Undo, exclusions, and no telemetry. Do not publish an unchecked binary. Physical Windows 11 UI, real user edit races, monitor sleep/lock and DPI acceptance remain separate.
