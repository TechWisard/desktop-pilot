# Direct shortcuts on both desktop roots — 2026-11-02

## Evidence and scope
Windows Explorer combines a user's Desktop with the common/public Desktop. Desktop Pilot scans both roots, but the current shared 6,000-shortcut budget is consumed recursively in the first root before the second one is visited. After the recent direct-before-nested improvement inside one root, a project folder containing 6,000 nested shortcuts on the personal Desktop can still prevent a directly visible common-Desktop shortcut from being discovered. This is a production control-flow observation, not a measured user failure rate.

Microsoft documents unspecified filesystem enumeration order: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findnextfilew . See the existing project studies of bounded catalog discovery and adapted interface predictability. User reports about desktop icon instability in other tools, for example https://forums.stardock.com/541785/issue-when-switching-virtual-desktops , motivate stable and unsurprising behavior but do not demonstrate that Desktop Pilot has the same issues.

## Target behavior
For personal and public Desktop roots, scan directly visible .lnk/.url shortcuts in deterministic root/name order first; only then enter permitted nested folders, and only after desktop discovery visit Start Menu roots. Preserve the same total cap, recursion-depth limit, reparse protection, exclusions, saved rows, manual rules, original bytes and Undo. No telemetry or new setting.

## Acceptance
Use the real production traversal on synthetic filesystem trees created in both orders. With two available slots and a personal root containing nested links plus one visible public-root link, before: 0 public links; after: 1 public link. Check unchanged deterministic result and strict total cap; verify Start Menu remains a lower-priority source. Run full Windows Server 2022/2025 Rust, Clippy, format, native integration and packaging guards. This does not establish human time savings or test a live Windows 11 Explorer session.
