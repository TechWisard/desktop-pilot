# Pending manual policy choices after a worker restart — 2026-11-01

## Existing evidence and source-confirmed defect

Continue the existing [manual-control research](2026-10-26-adaptive-shortcuts.md) and [queued-action ordering study](2026-11-01-queued-command-order.md) instead of repeating their search. Findlater and McGrenere's [CHI 2004 controlled menu study](https://doi.org/10.1145/985692.985704) establishes a reason to prioritize predictable user control; it does not measure this program or recovery after crashes. [A Fences forum report (June 2024)](https://forums.stardock.com/528482/icons-in-fences-get-rearranged-when-desktop-is-rebooted-or-refreshed) concerns another product and does not establish an incident here.

Source inspection confirmed a distinct recovery path: `Store::recover_commands` marks every pending command as failed at monitor startup, including policy changes the worker never claimed. A user choosing `Never` and then `Always` shortly before an interrupted background restart could lose both saved intentions without either being attempted. This is a confirmed code-path risk, not an observed frequency or a claim about human time saved.

## Production-path correction and regression

The worker-start recovery now retains **only unclaimed pending** `policy:` actions, preserving their SQLite IDs and FIFO order. Running actions are always terminal errors because they may already have had side effects. Pending Apply, Undo, rescan and other file-changing actions are cancelled; no automatic filesystem action is replayed. An explicit user-initiated cleanup from Settings continues to cancel everything when the worker is absent. Turning background work off still cancels all commands by its existing serialized shutdown path.

`src/engine/command_queue_order_tests.rs::unclaimed_manual_policies_survive_worker_restart_without_replaying_file_actions` exercises the actual Store/SQLite queue, a native-parsed `.url` shortcut, real worker-start recovery, the existing production dispatcher and policy handler, and explicit cleanup. A mixed sequence has a running action, pending Never, pending Apply and Undo, and pending Always. It asserts that only Never then Always execute once, without opening a layout transaction or touching source bytes. Repeated recovery does not duplicate warnings. The crash ordering is injected synthetically; file parsing, database commits and command execution are production code.

## Acceptance and limits

Run the unchanged Windows Server 2022/2025 workflow: locked Rust tests, formatting, strict Clippy, locked build, native integration and release guards. Verify the branch and merged main separately before release. A pending policy is a reversible catalog choice and does not itself add or remove desktop files; the user still presses Apply to alter layout. Physical Windows 11 Explorer behavior, a true process crash, high-DPI UI, and observed user-time savings remain untested. Do not release unverified work.
