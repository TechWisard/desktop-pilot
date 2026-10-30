# Preserve explicit exclusion explanations across source outages (2026-10-30)

## Evidence and priority

Baseline: Desktop Pilot 0.2.64, main `a9d3737fefc791673ff0d5f5ed2cd63f68127ce7`, public repository and green Windows run [41031611775](https://github.com/TechWisard/desktop-pilot/actions/runs/41031611775); no open PRs at inspection. Continue earlier evidence review in [adaptive-shortcuts.md](2026-10-26-adaptive-shortcuts.md) and [evidence-based-reasons.md](2026-10-28-evidence-based-reasons.md). Findlater and McGrenere (CHI 2004, https://doi.org/10.1145/985692.985704) support predictable user control in adaptive interfaces; that study is not a measured Desktop Pilot benefit.

Source-confirmed inconsistency: `model::rank` checked `!app.valid` before `app.policy == Policy::Never`. Thus, when a user explicitly marks a shortcut **Never add**, an offline original changes its displayed exclusion reason from **Excluded manually** to **Source shortcut or target unavailable**, although the explicit rule and eligibility are unchanged. This can incorrectly suggest that user intent was forgotten. It is a display-level logic error, not a claim that excluded shortcuts were placed.

## Production-path correction and acceptance

Give the deliberate Never policy priority in the reason calculation; retain the unavailable warning for Auto/Always as before. No new heuristics, settings, database schema, telemetry, background activity or sorting changes.

`model::evidence_reason_tests::explicit_never_reason_survives_missing_original_and_restoration` creates a real inert .url shortcut in an isolated Windows test desktop; saves the policy and usage in SQLite; obtains `engine::preview`; moves the original away; invokes the real `engine::refresh_apps`; checks the saved invalid state and the actual preview explanation; restores the source and rescans. Assertions cover Never priority before/during/after, no automatic additions, unchanged usage and original bytes.

Validation: run existing unmodified Windows Server 2022/2025 locked tests, formatting, Clippy, build, native tests, and publication checks. If the fix remains test-only or tests fail, do not publish a release. Physical Windows 11 at 200% scale, genuine disconnected network drives, long-horizon habit changes, human task-time, and input-hook CPU acceptance remain open in issues #22 and #56.
