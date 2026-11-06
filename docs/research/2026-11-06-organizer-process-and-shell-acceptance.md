# Organizer: process interruption, shell refresh and native interface acceptance

## Confirmed observation

The integrated Windows 11 run 42259459156 passed all 32 native executable checks, including real background stop/resume around organization. The subsequent live Explorer test moved both files but returned an `E_BOUNDS` (0x8000000B) warning during layout restoration. It failed acceptance; that warning was not ignored. Windows Server 2022/2025 passed their existing checks.

The desktop fixture's former requirement that its own window receive foreground was unnecessarily narrower than the application's contract. On the disposable Windows 11 runner, WinSta0/Default was already the active input desktop. A normal Show Desktop request and verification of an actual non-fullscreen, inactive foreground are sufficient. No production idle, keyboard or fullscreen guard was removed.

## Shell correction

`IFolderView::ItemCount` and `Item` are separate calls. An asynchronous view update can invalidate an index while a complete layout is being read. The post-rename settle loop previously propagated this read failure immediately instead of using its existing bounded refresh deadline. It now retries a complete layout read for the specific native `E_BOUNDS` error. No partial enumeration becomes a free-space snapshot. Access failures remain errors, topology/auto-arrange changes abort, and a persistent bounds failure remains an error after the existing three-second deadline. The live Windows 11 apply/undo check must still pass after this correction.

Primary references:
- https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-itemcount
- https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-item
- https://learn.microsoft.com/en-us/windows/win32/com/com-error-codes-1

The observed HRESULT is confirmed; the changing-enumeration mechanism is the source-based explanation being tested, not an assertion that every E_BOUNDS has this cause.

## Actual process termination

`process_recovery_tests` runs production Analyze/Apply in a child test executable, pauses at the existing post-rename checkpoint, and terminates that process after either the first or last rename. It does not return an injected error into Apply: normal in-process rollback cannot run. A new Store then opens the real SQLite/WAL profile, sees the durable pending operation, refuses unrelated catalog writes, and executes real Undo. File identities/bytes, notes, manual Always policy, usage history, settings and the disabled-background state are checked. Another Apply/Undo cycle proves continued usability. The child-only checkpoint is compiled under cfg(test), not into the product. These scenarios passed on Server 2022 and 2025 in run 42260132522; that revision still required two formatting corrections before a full green run.

This tests abrupt process termination, not a physical power cut, storage-controller failure or network/cloud outage. SQLite references: https://sqlite.org/wal.html and https://sqlite.org/pragma.html#pragma_synchronous .

## Native interface coverage

The capture script expands the existing Russian/English, 100%/200%, normal/filtered/files/warning views with bottom-scrolled 200% captures. Window-scoped WM_MOUSEMOVE/WM_MOUSEWHEEL messages are sent only to the explicit isolated ui-smoke child HWND after verifying its PID. The application handles those messages through its real native event loop. Captures use the existing verified app-local software renderer; they are not hardware-GPU evidence. A successful PostMessage count is not sufficient visual acceptance: inspect the resulting images and record whether the bottom controls are actually reachable and readable.

Coordinate references: https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mousemove and https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mousewheel .

## Remaining evidence boundary

Only a completed final combined CI and inspected screenshots may close these gates. Active OneDrive-account synchronization and physical multi-monitor/hardware testing are not supplied by this runner. Existing conservative handling of unsupported cloud/reparse objects and display-topology changes remains required. Synthetic fixtures are correctness evidence, not measured user time savings.
