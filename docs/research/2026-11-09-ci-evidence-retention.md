# Keep UI acceptance, avoid duplicate successful push evidence — 2026-11-09

## Observed repository cost

`.github/workflows/checks.yml` runs the same native organizer GUI review on Windows 2025 and Windows 11 ARM for both `push` and `pull_request`. Before this change the `Interface review evidence` step used `always()` on both events. Two successful events for one candidate each uploaded two almost-identical ~3.9 MB ZIPs (approximately 15.6 MB before one-day expiration), although both native acceptance executions already ran independently. The screenshot evidence is useful for human PR review, but duplicate successful push uploads do not add an independent visual state.

## Minimal retention policy

Keep the native GUI test execution **unchanged** in both events and on both Windows systems. Upload the resulting screenshot bundle on `pull_request` (whether tests pass or fail), or on a failed `push` for troubleshooting. A successful `push` still executes its UI test and all other release safeguards; it simply does not upload redundant visual evidence. The existing 1-day retention and native failure diagnostics are unchanged. No caching, billing, runner type, permissions, release paths, source-code/data capture or security policy changes.

GitHub Actions supports `github.event_name` and status-check functions in step conditions: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax and https://docs.github.com/en/actions/reference/workflows-and-actions/expressions.

## Regression acceptance

On the exact proposed head check both event types: `push` and `pull_request` must each pass Rust format, locked tests, strict Clippy, build, native integration, live Explorer/cloud fixtures and native egui screenshots across the existing Windows matrix. On a successful push, the `Interface review evidence` step must be skipped and the run should create **zero screenshot ZIPs**. On the PR, the same step must succeed for Windows 2025 and ARM and both archives must be available for review. Failure-path uploading is retained by the status expression and the existing diagnostics; don't simulate failures by disabling a required check.

This is an operational-storage improvement, not proof of faster UI, a new product feature, or grounds for a 0.3.0 release. Preserve its physical acceptance gates.
