# Command cancellation feedback from SQLite to Settings (2026-10-30)

## Motivation and provenance

Continue the completion/cancellation correction from PR #66 and `docs/research/2026-10-30-terminal-command-outcomes.md` rather than revisiting old reports. The previous Windows regression verified that a delayed worker response could overwrite a cancellation in SQLite. A separate user-facing boundary remains important: Settings must display the **cancellation**, not a false success, and then accept a genuinely new successful command. This is a targeted acceptance hypothesis about production UI feedback, not a measured customer complaint.

The existing study by Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) motivates preserving explicit user control and predictable feedback. It does not establish a numerical user time saving. The relevant primary implementation is the actual `Store::recover_commands` / `Store::finish` transaction and `Settings::reload` notice logic.

## Production-path fixture

`src/ui/terminal_command_feedback_tests.rs` opens an isolated real SQLite profile and actual `Settings`. It queues and claims a manual rescan, cancels via a second Store connection's startup recovery, delivers a stale success, and reloads the same real Settings view. Assert that the pending marker disappears, the user sees cancellation/error, command event count is unchanged, and no application state or config is modified. Then claim/finish an independent new rescan and assert that its successful confirmation is shown. No shortcut on the real desktop is touched.

Unlike PR #66's database-only tests, this follows the outcome to the actual UI state. It is a deterministic integration fixture, not physical keyboard/mouse or Windows 11 timing evidence.

## Gates and limits

Run the existing Windows 2022/2025 `cargo fmt --check`, `cargo test --locked`, strict Clippy, locked build, 29 native checks, README and nine publication guards. Do not bump the package version for a test-only addition or publish a no-op binary. The last shipped 0.2.62 remains the release boundary until a separate verified functional change warrants a new version.

Remaining acceptance: physical Windows 11 at 200% DPI, first-result latency with 5,000 shortcuts, actual network-drive stalls, multiday habit changes and input-hook CPU measurement (issues #22 and #56). No synthetic human task-time claim.
