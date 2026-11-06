# Desktop organization: native acceptance and Explorer refresh

## Scope

Integrate organizer 9424d6f3eed20df8707c15e2858b340b6f46041f with stable 9b14123b1332e4b1d9c469ea4a26ae128377c432, preserving both histories. PR #211 replaces the long-running integration gap in #98. Product source merged without textual conflicts; CI configuration was reconciled without removing existing checks.

## Confirmed observations, not assumptions

Run 42200061062 executed the production x64 program on Windows Server 2022/2025 and compiled and executed its Rust suite on Windows 11 Enterprise 10.0.26200 (the standard public windows-11-arm runner, image 20261101.203.1). Windows 11 executes this x64 target through system emulation; this is not a physical x64 hardware or performance benchmark.

The new disposable-desktop acceptance test moved two real .url files through the production organizer. Windows Server 2025 returned a safe-files warning because Explorer had not yet completed enumeration of the new folder and/or removal of its old children. Existing filesystem fixtures had not exercised this shell boundary. The test intentionally rejected the warning rather than silently treating the operation as fully successful.

Five old adaptive-review scenarios on Windows 11 supplied an initial idle value but implicitly read the runner's real keyboard/window state at the later permission boundary. They now use the existing test-only boundary injection for that same production path. Runtime idle/fullscreen/held-key checks were not changed or disabled. Explicit user-return boundary regressions remain in the ordinary suite.

## Primary sources

- Microsoft, SHChangeNotify: https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify — SHCNF_FLUSH waits for notification delivery. It does not promise that the view has already enumerated every file before the next independent query.
- Microsoft, supported desktop positioning API: https://devblogs.microsoft.com/oldnewthing/20211122-00/?p=105948 — IFolderView::SelectAndPositionItems remains the supported route.
- Microsoft, IFolderView::SelectAndPositionItems: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-selectandpositionitems
- GitHub hosted runner reference: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- Windows x64 emulation: https://learn.microsoft.com/en-us/windows/arm/apps-on-arm-x86-emulation

The existing organizer research corpus remains the basis for explicit consent, predictable layout and reversible operations. No new claim about real-user time savings is made.

## Fix and regression design

After a verified rename batch, poll the real shell layout for the expected appearances and disappearances, with a three-second monotonic bound and 50 ms polling. A new monitor topology or auto-arrange change aborts positioning immediately. Timeout retains the actual latest occupancy; the existing collision and warning behavior remains authoritative. An object recreated by the user is a real obstacle, never a stale icon to ignore. No unrelated icon is repositioned.

Four deterministic regression sequences use the production polling path: staggered folder/child updates, timeout preserving stale occupancy, monitor/auto-arrange changes, and Undo enumeration. The opt-in live Explorer test additionally verifies real file identities, bytes, original icon coordinates, unchanged unrelated icons, the completed journal and disabled-worker choice.

Native executable self-test now exercises actual worker stop/restart around Apply and Undo, preserves newer settings and manual rules, and respects a newer Turn off choice. This is separate from unit tests that can only hold a test mutex.

Native UI capture runs the real settings executable in isolated profiles, in Russian and English at 100% and 200%, with normal, filtered, optional-file and changed-preview states. It refuses capture against a real or external desktop. Captures require visual review; file existence alone is not a usability verdict.

## Release record

The initial red run is preserved above. Final successful run IDs, reviewed captures, source/version/tag and asset hashes are recorded in the release acceptance report and PR #211 after they are actually verified. No gate is waived by this document.

Multi-monitor physical hardware, an active OneDrive cloud-sync account and third-party Explorer replacements are not reproduced by these isolated runners. Network desktops and unsafe cloud/reparse objects retain their existing conservative exclusions. No user's desktop is accessed by the tests.
