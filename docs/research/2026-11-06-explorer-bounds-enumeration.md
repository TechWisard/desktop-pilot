# Explorer enumeration during desktop organization — 2026-11-06

## Observed

The real x64 organizer, running under Windows 11 ARM64 emulation on a free hosted runner, moved two synthetic `.url` fixtures but reported the shell warning `0x8000000B` (E_BOUNDS) during Apply. The full Rust suite passed (504 tests), while the separate live Explorer Apply/Undo acceptance failed. See [run 42259459156](https://github.com/TechWisard/desktop-pilot/actions/runs/42259459156), Windows 11 job. Windows Server 2022/2025 had previously passed. This is an actual shell-boundary failure, not evidence of user-file loss or a user-facing release success.

The view is enumerated through IFolderView::Item and GetItemPosition. Explorer may change its item list between ItemCount and Item calls, so a transient out-of-range view index is a plausible cause; the exact failing method has **not yet been identified**. The foreground lock hypothesis from earlier diagnostics did not explain this new error.

## Change and acceptance

The existing three-second read-only shell-settling poll now retries only E_BOUNDS from a view read; on expiry the original error is returned. Other failures, topology changes, active auto-arrange and genuine duplicate occupancies remain hard stops. Tests exercise a one-time shell error followed by a recovered real-layout snapshot, persistent E_BOUNDS until timeout, and an unrelated error that must never retry. Method-level context is added to native enumeration and positioning so the next failing run identifies the actual boundary.

These are synthetic regressions exercising the **production** polling path, not proof of time saved by real people. CI must execute them on Windows. The live Explorer round-trip, actual undo, 200% UI, OneDrive and multi-monitor acceptance remain independent requirements; a green unit test does not waive them.

## Sources

- [Microsoft: IFolderView::Item](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-item)
- [Microsoft: IFolderView::GetItemPosition](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-getitemposition)
- [Microsoft: IFolderView::SelectAndPositionItems](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-selectandpositionitems)
- [Microsoft: SHChangeNotify](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify)
