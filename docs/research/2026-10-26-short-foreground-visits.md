# Repeated short foreground visits: preserve useful evidence (2026-10-26)

## Evidence and limits

- Czerwinski, Horvitz, and Wilhite, *A Diary Study of Task Switching and Interruptions* (CHI 2004), report task interleaving and interruptions among information workers: https://www.microsoft.com/en-us/research/publication/a-diary-study-of-task-switching-and-interruptions/
- Tanaka and Fujita, *Study of User Interruptibility Estimation Based on Focused Application Switching* (CSCW 2011), use observed application switching as a signal in an interruptibility study: https://doi.org/10.1145/1958824.1958954
- Findlater and McGrenere, *A Comparison of Static, Adaptive, and Adaptable Menus* (CHI 2004), found that participants generally preferred user-controlled customization, motivating caution about automatic rearrangement: https://doi.org/10.1145/985692.985704
- These papers establish that switching and interface stability are relevant HCI concerns. **They do not establish that a ten-second threshold or a twenty-minute memory window is optimal for Desktop Pilot.** No independent user telemetry or human timing study was collected.

## Confirmed source behavior

The production `UsageAccumulator::sample` previously discarded its entire state whenever the foreground application ID changed. A six-second confirmed visit contributes only four seconds after the deliberately ignored switching sample. Repeating that visit to the same application several times, separated by work in other windows, still contributed **zero** recorded usage even after cumulative confirmed activity exceeded ten seconds. Extended uninterrupted sessions were already handled correctly. This mattered to quick references, messages, and other short-use workflows.

## Implemented behavior and safeguards

- Maintain up to 128 **in-memory** per-app partial usage windows, with a twenty-minute inactivity expiry. Do not change SQLite format, telemetry behavior or config defaults.
- The first sample after a foreground switch is still excluded. Only consecutive confirmed samples with valid monotonic elapsed time add to that app's own window. Switching to a different app does not transfer usage between identities.
- After ten confirmed cumulative seconds for the same app, report them through the **existing** production `record_usage` and SQLite observation path. Keep the original session-gap rule and the ranking/manual-placement policy.
- Invalid, suspended, nonfinite or >=5-second timing samples clear pending evidence; visits separated by twenty minutes do not join. An unused entry expires automatically, and the in-memory catalogue is bounded to 128 identities.

## Reproducible regression scenarios

1. **Repeated short visits:** alternate three six-second visits to each of two apps. The first tick of each visit is ignored; each app contributes 12 confirmed seconds and exactly one session into the real usage store. Before the change, both contributed zero.
2. **False-positive defense:** rapidly alternate single ticks; neither app should qualify. After a partial visit and more than twenty minutes of inactivity, three fresh two-second ticks remain insufficient. A simulated 600-second stalled tick resets pending evidence.
3. **Memory bound:** cycle over 300 distinct app IDs with short visits, verifying the per-app memory never exceeds 128 entries.
4. **Existing long-session regression:** an uninterrupted active window crosses ten seconds and continues to count in real elapsed increments; elapsed sleep time is discarded.

The tests exercise the actual accumulator and, where applicable, production `record_usage` and `Store::days`. They are synthetic deterministic sequences, not proof of measured speedups or human demand.

## Verification and follow-up

Run repository Windows CI (`cargo fmt --check`, locked test suite, Clippy, build, native fixtures) on the branch before merging. No physical Windows Explorer, sleep/resume hardware or field-user sessions are available in this environment. Follow-up should evaluate the 20-minute retention period and false-positive/false-negative tradeoffs against opt-in real-user studies; do not silently add user telemetry. Preserve stable placement and manual rules over automatic suggestions.
