# Explorer icon-position read failures — 2026-11-07

## Verified baseline

[Windows checks run 42300122251](https://github.com/TechWisard/desktop-pilot/actions/runs/42300122251) compiled the integrated desktop organizer and passed the Windows Server 2022 and 2025 jobs. The Windows 11 ARM hosted runner passed 32 native integration checks, then failed the separately required live Explorer acceptance. During Undo, the real file operation restored two synthetic `.url` files, but Explorer's `IFolderView::GetItemPosition` returned `E_FAIL (0x80004005)` at icon 7/8. The outcome recorded a shell-position warning, and the test correctly failed rather than claiming restored coordinates. This is **not** evidence that original files were lost.

[Earlier Windows 11 run 42259459156](https://github.com/TechWisard/desktop-pilot/actions/runs/42259459156) reported `E_BOUNDS` during the same multi-call shell enumeration. The code already retries that error in the three-second organizer settling loop. It did not recognize the distinctly observed `GetItemPosition` `E_FAIL`.

## Hypothesis and bounded correction

`ItemCount`, `Item`, and `GetItemPosition` are distinct COM calls; the desktop folder view can change between them after shell notifications. A stale position query is a plausible explanation, **not yet proven** to be the cause of this E_FAIL. The code now marks `E_FAIL` as a *potentially stale view* **only** at `GetItemPosition`, rather than treating arbitrary COM failures as harmless. All desktop item reads use a short, bounded (400 ms) complete-view retry; Apply/Undo settling additionally retains its existing three-second bound. Every attempt starts fresh at `ItemCount`. We do not discard an icon, invent a position or accept a partial occupancy map. Persistent E_FAIL, access failure, changed topology and auto-arrange remain hard stops for positioning.

Regression cases on the production settling/read-retry helpers cover a recovered complete view with an unrelated icon, a persistent failure bounded by its retry budget, and an unrelated E_FAIL which must not retry. These are synthetic failure injections; they do not assert human time savings or prove the hosted shell behaves as hypothesized.

## Acceptance record

- Baseline run: 42300122251, Windows 11 ARM live Explorer acceptance **failed** at Undo while Windows Server 2022/2025 jobs **passed**.
- Candidate branch: `fix/explorer-transient-position-read-20261107`. CI and a repeated Windows 11 Explorer Apply/Undo must establish whether the correction actually resolves the observed failure.
- **Do not merge or release** the desktop organizer on the basis of unit tests alone. Windows 11 UI review at 100%/200%, interrupted-operation recovery, redirected/OneDrive desktop and physical multi-monitor behavior remain separate gates.
- No changes to user files, local history, privacy, telemetry, security settings or billing are intended.

## Primary API references

- [Microsoft, IFolderView::Item](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-item)
- [Microsoft, IFolderView::GetItemPosition](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-getitemposition)
- [Microsoft, IFolderView::ItemCount](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-itemcount)
