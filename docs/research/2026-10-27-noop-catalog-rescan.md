# No-op rescans of stable shortcut catalogs — 2026-10-27

## Evidence and scope

Baseline: released Desktop Pilot 0.2.22, commit `3aa53990ecec1c5b398697f6e92285b5eb3b72ce`. This continues the catalog cold-start investigation in [issue #22](https://github.com/TechWisard/desktop-pilot/issues/22) and [catalog-snapshot.md](2026-10-27-catalog-snapshot.md); it does not repeat the earlier adaptive UI literature survey. [Flow Launcher #4484](https://github.com/Flow-Launcher/Flow.Launcher/issues/4484), opened 2026-05-21, is one *independent user report* that saved custom shortcuts became unavailable after restart; it does not prove Desktop Pilot has this problem. [SQLite's UPSERT documentation](https://www.sqlite.org/lang_upsert.html) documents the `INSERT ... ON CONFLICT DO UPDATE` mechanism used by `Store::put_app`.

## Source-confirmed behavior and hypothesis

In the released `engine::refresh_apps`, every known app is loaded and checked against the real original shortcut, then `store.put_app(&app)` is called even if none of its fields changed. `pins_verified` is also persisted on every rescan despite remaining the same. These redundant UPDATE operations may create additional SQLite writer contention on large catalogs; *battery or user interaction time has not been measured*. The narrow goal is to keep identical records untouched while retaining real changes and user-authored fields.

## Red-first regression plan

`src/store/catalog_rescan_tests.rs` creates isolated actual Windows `.url` files and uses the production `Store`, `engine::refresh_apps`, `apply`, `preview`, `rollback` and the real SQLite `total_changes()` counter; it does not replace the scanner or ranking algorithm.

1. After filling a 512-entry shortcut catalog, mark one rare app Always, record use, create its quick copy and repeat five unchanged rescans. Expect **zero SQLite row writes** after initial scan, **zero proposed placement changes**, unchanged history, original and managed bytes, positions and usable Undo.
2. Modify only the icon path/index of one original in a 64-entry catalog. Expect exactly one app-row update, new metadata, no changes to policy, history, managed copy or Undo, and zero writes on the next identical rescan.
3. Temporarily remove an Always source in a 32-entry catalog, repeat rescans, restore the file and check the catalog settles without repeated writes or deleting the protected quick copy.

The initial test-only branch intentionally leaves production unchanged. The existing no-op scan should fail the write-count assertions while valid shortcut parsing and actual Undo continue; formatting and compilation must pass for the failure to count as behavioral evidence.

## Acceptance and verification

After a narrow correction, run unchanged `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, native integration fixtures and publication guards on Windows CI for both existing runners. Compare exact SQLite write counts before/after, not synthetic wall-clock figures. Keep profile/schema, filename handling, manual policies, source files, coordinates, history, Undo, and known app identity unchanged. Do not release unless CI passes. Real Windows 11 Explorer startup, a cloud sync client and actual human task latency remain separately unverified.
