# Bulk import and first saved search (2026-10-28)

## Starting point and primary evidence

Continue from released 0.2.37, `6c27fe636ad4b727a935763ae9ab04463d5662b5`, main CI 40700529755 successful. No open PR at inspection. Earlier work on duplicate shortcut parsing (PR 39), indexed scanner identity lookup, saved UI snapshots, ownership, manual import feedback and undo remains intact. This investigation extends issue 22; it does not repeat a Windows-version compatibility exercise.

- Existing adaptive-interface research: Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004), https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/. Preserve explicit choices and stable placement rather than pursuing faster destructive rebuilding.
- Existing analogous report: Flow Launcher issue 4484, https://github.com/Flow-Launcher/Flow.Launcher/issues/4484. A user reported delayed availability of a custom shortcut catalog after restart; the issue is closed. As already documented in issue 22, the reporter later described apparent recovery after plugin updates. This is motivation for testing, not proof of an unresolved Flow Launcher defect or a Desktop Pilot delay.
- SQLite primary documentation: https://www.sqlite.org/lang_transaction.html, section 2.1, permits multiple readers but only one simultaneous writer. Unnecessary computation inside an immediate write transaction can therefore also delay unrelated writers. This is a mechanism, not a measured real-user delay.

## Source finding and hypothesis

The discovery scanner already has indexed launch identities, but Settings manual import still called `prepare_imported_app` for each new parsed shortcut. With no exact saved ID, that function recomputed the full launch identity of every existing app. The saved map grows within the same batch. Therefore importing N distinct new shortcuts requires quadratically many identity evaluations while the SQLite write transaction and existing changes lock are held. This is separate from decoding each source file and was not fixed by PR 39.

The correction builds one transaction-local launch-identity index alongside the authoritative ID map. It preserves all matching IDs per identity, rejects ambiguous legacy matches, and retains exact-ID precedence. Updates keep the index synchronized. The original ownership guard, changes lock, SQLite transaction, per-item errors, import counts, revision notification and rollback behavior are unchanged. The index is ephemeral; no new schema, dependency, configuration, telemetry or persistent cache is introduced.

## Executable evidence

`src/ui/import_scaling_tests.rs` calls the actual asynchronous `Settings::add_shortcuts`, native URL parser, real SQLite store, Settings constructor/search/preview and actual Undo. The existing `App::identity` counter is read on the real import thread. The metrics hook and registry are test-only and retain diagnostics only for registered fixtures.

1. Import 100, 1,000 and 5,000 real inert URL files into isolated profiles already containing a manually protected shortcut and history. Record total import time, transaction duration (including acquisition), and actual identity evaluations. Check exact entry counts, no changed rules/history/positions, no duplicate identity, no-op repeated import and one revision per changed batch. Construct fresh Settings connections and search the saved catalog with the optional source directory both present and renamed offline. Verify no unexpected placement and functioning Undo without deleting any originals.
2. Mixed legacy-ID originals, equivalent duplicates, new duplicates and a managed quick copy retain one catalog identity, Never, notes, usage and the original quick-copy source.
3. Ambiguous legacy identities produce an error without blocking an independent valid import. An exact saved ID still takes precedence, preserving the previous behavior and both legacy records.
4. A real SQLite trigger fails a batch after an earlier entry was inserted. Assert that the entire catalog, revision, settings, ownership and history roll back; remove the fault, retry, and verify accurate counts and a no-op repeat.

Before/after uses the same fixtures and tests, serially on one Windows Actions runner. The sole intended red assertion is the linear identity-work bound; all correctness boundaries must pass on both versions. Exact measured outputs and commands are saved in `2026-10-28-bulk-import-scaling-validation.json` and its linked run. Full ordinary PR/main checks remain mandatory; preparation is not publication.

```powershell
cargo test --locked ui::import_scaling_tests -- --test-threads=1 --nocapture
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
./scripts/test-native.ps1
./scripts/test-release-workflow.ps1
python scripts/test-readmes.py
```

For baseline reproduction, start at the recorded baseline commit, copy the new test module, expose the existing `with_profile` fixture to sibling tests, and apply only the `#[cfg(test)]` instrumentation/module wiring from the final UI diff. Keep the original production import helper and HashMap. Run the same command; a compiler failure is not performance evidence.

## Limits and continuation

Diagnostic timings are one paired debug-build CI run, affected by runner load, filesystem caches and instrumentation. They are not cold-disk-cache or full process-start measurements, human task-time savings, power measurements, or network performance guarantees. Saved search is measured through a newly opened Settings/SQLite connection in the test process. The ordinary UI geometry/input tests still cover scaling and readability; no physical Windows 11 or manual 200% DPI interaction is claimed.

Keep issue 22 open for real Windows 11 input-to-render and first-launch measurements, real stalled network/profile drives and long-horizon project changes. Extend production-path tests only when new behavior warrants it; do not repeat these baseline searches or replace manual rules with inferred preferences.
