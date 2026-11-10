# Paused learning: truthful Home guidance and direct recovery — 2026-11-09

## Source-verified user scenario

An existing local profile can have `automatic=false`, `collect=false`, `learning_enabled=true`, and incomplete `Progress` (including legacy profiles or a saved manual learning preference). The real Home implementation reports `Observation paused`, but the empty-shortcut card said the first shortcuts would appear *after observation* even though no new observations are collected. The existing **Open settings** action selected Settings' default Mode subsection, not the Observation subsection containing the blocking requirement. Applying remains deliberately gated until the user changes the learning setting or completes observation; this change does not silently waive that consent.

The discrepancy is reproducible from `Settings::new`, the saved SQLite `Config` / `Progress`, `Settings::overview`, and production egui input. It is a confirmed code-level UI mismatch, **not** a customer-reported defect.

## User research boundary

Gajos et al., *Predictability and accuracy in adaptive user interfaces* (CHI 2008), https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml , found that accurate and predictable adaptive interfaces improve satisfaction and use. Findlater and Gajos, *Design Space and Evaluation Challenges of Adaptive Graphical User Interfaces* (2009), https://www.eecs.harvard.edu/~kgajos/papers/2009/findlater09design.shtml , recommend evaluating task choices and actual user outcomes. Neither study tests this application's status text or measures time saved by this change. The research already catalogued for this project is reused; no repeat of broad prior searches.

## Change and before/after

In a paused-learning profile with no shortcuts, Home now says that no data are being collected and identifies **Settings → Observation** as the exact place to resume collection or turn off the initial learning requirement. The existing **Open settings** control in the same status card opens that subsection directly instead of the unrelated Mode subsection. This removes one navigation step in the tested scenario and eliminates the impossible promise of automatic progress. It does not add an extra button or crowd the main view.

The regression uses a real temporary SQLite profile, both RU and EN, incomplete learning progress, an actual egui Home paint and pointer click. It asserts both the new visible explanation and the selected Settings subsection, then checks that persisted Config, learning sessions and jobs are unchanged. Other status reasons retain their precedence; no collection is started, no shortcut is moved and no changes are applied.

## Release boundary

Keep on unreleased 0.3.0 organizer integration until full Windows CI and independent physical Windows 11 multi-monitor/DPI, active OneDrive, keyboard/accessibility and interrupted-power acceptance. Synthetic egui pointer verification does not replace a human keyboard usability session or prove human productivity savings.
