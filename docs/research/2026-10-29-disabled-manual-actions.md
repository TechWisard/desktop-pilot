# Explicit manual actions while tracking is disabled — 2026-10-29

## Starting state and evidence

At inspection, main was the public 0.2.44 release commit e61c92486e032790ccc598cd7c0ea5f20b497178, with successful standard Windows 2022/2025 CI and no open PRs. TechWisard/finite-checks was not found; no separate test repository is required because the product's own public CI is available. Earlier research about predictable adaptive interfaces and user control is retained in docs/research/2026-10-26-adaptive-shortcuts.md and later combined use/Undo coverage in docs/research/2026-10-29-project-change-undo-acceptance.md.

The source-confirmed defect is an interaction between Settings::queue and setup::start_background: **every** explicit settings action called start_background before enqueuing a command. That method installs/updates and starts the background executable, then clears background_disabled. However setup::disable_background deliberately sets background_disabled = true and stops collection, automation, and autostart; the Settings UI tells users those remain off. A subsequent manual Never/Always or Apply/Undo could therefore unexpectedly resume the worker, or fail because an installed executable could not be repaired. This is independent of algorithmic recommendations or Windows-version compatibility.

The earlier primary human-interface research by Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) motivates predictable user control; it is not evidence of a specific complaint about Desktop Pilot or of human time savings.

## Reproduction on production paths

src/ui/disabled_actions_tests.rs calls the real asynchronous Settings::queue, persistent SQLite Store, engine::command, actual inert URL shortcut and file-level Apply/Undo. It first disables the profile via setup::disable_background, then deliberately obstructs the installed executable path with a directory. Explicitly changing Never, applying the removal and Undo must succeed **without** starting the background worker, rewriting an original shortcut, losing usage history, or clearing the disabled/shutdown flags. The deliberately obstructed executable makes an unwanted installer attempt fail deterministically instead of launching a background child during a failure-first test.

The initial test-only run with unformatted fixture was not behavioral evidence and must not be counted as such. The formatted baseline still contains the original production queue. Distinguish behavioral failure in that run from the corrected Windows CI.

## Correction

The Settings action entry point uses setup::start_background_if_enabled to serialize the choice with explicit Turn off/start under the existing startup lock. Enabled profiles retain the original worker/command-queue flow. Explicitly disabled profiles instead run the requested command directly, on an asynchronous job thread with COM initialized and the existing changes/SQLite/Undo safeguards. The direct path verifies the worker is not still running, does not alter background_disabled or restart the installer, and reports success/errors through the Settings job result. Explicit resume still calls the original setup::start_background and requires a separate user action.

No new permission, background thread, data schema, setting, external service, dependency, or telemetry is introduced. Other mode and ownership tests must still pass.

## Remaining limitations

The fixture verifies one combined local manual-operation sequence; it does not measure human productivity, validate physical Windows 11 screen input at 200% DPI, or emulate a real network outage. Preserve the outstanding cold-start/long-horizon acceptance issue #22. Do not treat a red format-only run or an isolated green test as a release check. Publish only after normal PR and main/release jobs pass and all release assets are verified.
