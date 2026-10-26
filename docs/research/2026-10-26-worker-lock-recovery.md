# Recovery from temporary database contention and visible worker errors — 2026-10-26

## Evidence and engineering decision

Primary sources:
- SQLite distinguishes `SQLITE_BUSY` and `SQLITE_LOCKED` from permanent I/O, permission, corruption and disk-full errors. A write transaction may fail because another connection holds a conflicting lock: https://www.sqlite.org/rescode.html and https://www.sqlite.org/lang_transaction.html
- Microsoft Windows desktop error-message guidance recommends stating a consequential failure in the interface, explaining the problem and providing recovery actions rather than relying on hidden logs: https://learn.microsoft.com/en-us/windows/win32/uxguide/mess-error
- Findlater and McGrenere (CHI 2004) studied static/adaptive/adaptable menus and user preference for control and consistency: https://doi.org/10.1145/985692.985704
- Anecdotal issue from an adjacent launcher: a Flow Launcher user reported an unexpected application crash during normal usage (July 2025): https://github.com/Flow-Launcher/Flow.Launcher/issues/3835. Its underlying cause is **not** established to be the same.

These sources motivate a bounded retry for database contention and a clearly visible stop reason. They do **not** quantify how frequently Desktop Pilot is affected or prove a human time saving.

## Actual implementation before the patch

- `engine::monitor` used `record_usage(...)?`, so even a single write-lock error after SQLite's existing timeout terminated the entire background monitor. The top-level worker wrote a stopped-error status/event if the database could be opened, but no retry occurred.
- `ui::health` correctly classified a dead worker as `Offline`. The Home view's `Health::Offline` branch *unconditionally hid the stored error text*, showing only a generic explanation and Resume action. Users could miss the cause and repeatedly restart a broken worker.

## Minimal change and safeguards

- Retry **only the complete, transactional usage write** when the underlying SQLite error is `DatabaseBusy` or `DatabaseLocked`. At most three attempts, with 125 ms between attempts after SQLite's own busy wait. A successful transaction increments usage exactly once; failed attempts leave both SQLite history and in-memory session markers unchanged (existing transactional safeguard).
- Keep persistent failures visible, never retry corruption, storage exhaustion, permission errors or unexpected errors, and never retry desktop-file mutations. No extra permissions, network traffic, telemetry, schema changes or background services.
- When the worker is *not running* and was *not explicitly disabled*, Home now shows its existing persisted `Background tracking stopped:` error (only the first line) with both Resume and a History link. Suppress stale running/paused status, errors from a *running* worker and errors after intentional disabling.
- Display the explanation within the existing Home health card; do not add a new panel or controls to normal healthy operation.

## Executable regression scenarios

- With a **real second SQLite connection** holding `BEGIN IMMEDIATE` and the production `Store` timeout reduced to 1 ms for the fixture, fail the first real `record_usage` attempt, release the lock, then retry the same production transaction. Assert two attempts, 10 persisted seconds, exactly one session and the correct memory checkpoint.
- Hold the SQLite write lock for all three attempts. Assert bounded failure, unchanged database usage/session state and a successful later retry after release.
- Inject a real SQLite `RAISE(ABORT)` trigger into the session checkpoint update. Assert it is attempted exactly once (not mistaken for transient contention), rolls back `days` and does not advance the cached session.
- Check the same worker stop markers read by Home. A stopped-error message is surfaced while a healthy stale message, disabled state and running state are not misreported.

The sequences are deterministic synthetic fault injection into **real production SQLite code**, not a stand-alone recommender model. They do not reproduce a physical full disk, physical Windows Explorer or a field-user failure.

## Next work

1. Use an isolated Windows UI screenshot/profile fixture to visually check the Home error state at 150–200% scaling and in English/Russian. Current automated helper tests do not demonstrate pixel-level layout.
2. Test a real long-held SQLite lock while the complete background process/tray are running, including restarting and resuming a queued command. The unit fixtures exercise the production transaction/retry path, but not the full process.
3. Consider a recovery policy for database errors in other worker operations only after verifying those calls are safe to retry. Never blanket-retry desktop changes.
4. Continue the long-sequence project-switching, rare-app, manual placement and five-day absence scenarios from earlier research. No user telemetry is introduced.
