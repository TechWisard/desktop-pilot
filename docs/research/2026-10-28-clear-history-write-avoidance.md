# Clear history without rewriting unchanged shortcuts (2026-10-28)

## Starting point and evidence

Continue from released Desktop Pilot 0.2.41, main commit `bbbc471c58dafef81605b9a14ebd6e118279ddea`. The existing repository is public, ordinary free GitHub-hosted Windows checks succeed, and no PR was open at inspection. A separate `TechWisard/finite-checks` repository was not found and is not needed. Previous adaptive-interface and cold-start research remains in `docs/research/2026-10-26-adaptive-shortcuts.md` and issue #22.

The production `engine::command(store, "clear_stats")` cleared daily history, then deserialized every saved shortcut, reset two optional historical-seed fields and called `Store::put_app` **unconditionally** for every entry. Imported applications ordinarily have both fields already zero. `put_app` uses an SQLite UPSERT that updates the row even if serialized data is unchanged. The whole operation takes place under an SQLite immediate write transaction. Large catalogs can therefore keep the writer busy with pointless disk operations when the user chooses **Clear history**; this is directly verifiable database work, not evidence of real-world response time.

SQLite's primary documentation on connection-local row changes: https://sqlite.org/c3ref/total_changes.html. Prior UI research (Findlater and McGrenere, CHI 2004, https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/) motivates preserving user-authored rules and stable layout while optimizing; it does not provide any Desktop Pilot time savings.

## Failure-first reproduction on real code paths

At branch commit `7bf4e1c64ccf38cc2e30f6c98ce63df84f4fd72b`, before modifying the production clear-history loop, `src/ui/clear_history_tests.rs` added two tests using a real `Settings` profile with one manually protected, placed and undoable shortcut, its usage history, and 1,000 further native-parsed inert `.url` originals. Among the batch, one has a historical seed and one an explicit Never rule and note.

[Failure-first Windows Actions 40742639992](https://github.com/TechWisard/desktop-pilot/actions/runs/40742639992) passed formatting and **failed only the new write-budget assertion on both Windows Server 2022 and 2025**: the first clear recorded **1,004 changed rows** for 1,001 saved apps, while 280 other Rust tests passed per runner. The companion forced-SQLite-failure/rollback/retry test passed on both. These are actual SQLite change counters from the production command, not a separate modeled algorithm.

## Correction and safety

Only call `put_app` when an app has a nonzero historical seed or seed timestamp. Clearing actual daily history, learning state and session checkpoints remains transactional and unchanged. No new table, configuration, telemetry, dependency or external resource. The regression protects the Never/Always rules, notes, configuration, existing ownership/position, original and quick-copy bytes, exact catalog count, empty usage history after success, repeat clear without unnecessary app writes, functioning Undo, and recovery from a deliberately failed database update. The forced failure must roll back the entire operation without losing history.

Run the unchanged `cargo fmt --check`, `cargo test --locked`, strict `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, `scripts/test-native.ps1`, release guards and README checks on both normal Windows runners. The release workflow must separately rebuild and verify the distributable before publishing.

## Limits and continuation

The row-change reduction indicates less redundant SQLite work and a shorter potential lock window; it does **not** directly measure perceived task completion, responsiveness on physical Windows 11, energy or user productivity. Continue issue #22 with actual cold-start and input-to-render timing at 100%/200% DPI, real network/profile outages, and longitudinal habit/project transitions. Do not turn a green synthetic test into a claim of universal perfection.
