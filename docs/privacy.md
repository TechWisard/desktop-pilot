# Privacy

[Русский](privacy.ru.md) · [Home](../README.md)

Desktop Pilot has no telemetry, account system, synchronization or application network requests. Opening an original shortcut can of course launch an application that uses the network.

Data remains in `%LOCALAPPDATA%\DesktopPilot`:

| Data | Purpose / retention |
| --- | --- |
| App names, source/target paths, arguments, working directories, taskbar-pin status | Identify and launch shortcuts; retained in the catalog |
| Confirmed sessions and active seconds; legacy launch counters | Sessions and seconds rank apps; launch counters do not. Usage rows older than 90 days removed during maintenance |
| Rules, configuration, managed shortcut paths and positions | Preserve your choices and desktop layout |
| Initial learning progress, qualified session count, last activity timestamp and daily active seconds | Enforce the passive-observation threshold; up to 90 day totals, no sequence of app/window events |
| Operation events | Latest 2,000 events retained during maintenance; latest 100 shown |
| Last counted session time for each app | Prevent restart from counting the same session twice; entries older than 90 days removed during maintenance or when a session is recorded; cleared with usage history |
| Command results | Diagnose requested actions; the latest 2,000 commands are kept during maintenance, together with any unfinished commands |
| Transaction journal and shortcut backup bytes | Undo changes; retained until you remove local data |
| Last error text | Diagnose the latest failure; replaced on another failure |

The tracker reads the foreground process identity and Windows idle duration. Low-level Windows input hooks inspect only event type and injected-input flags. A volatile record holds the latest qualifying interaction timestamp, serial number and destination window handle. Key codes, text, button identifiers and cursor positions are never persisted; mouse positions are used transiently only to check which window received the interaction. Collection disabled means no new interaction metadata is retained. It does not record keys, window titles, document contents, browsing history, audio or screen images. The explicit diagnostic `--ui-smoke` command, restricted to isolated `test-*` profiles, saves images of the app's own settings window to a requested directory; normal operation never invokes it.

Pausing **Collect usage** stops new observations. **Clear usage history** removes daily counters and bootstrap usage scores, while keeping shortcuts, rules and undo records. Pending initial-observation progress is reset; completed learning remains completed. Turning off automatic changes does not itself stop collection. Exiting the tray process stops both until the app starts again; disable Windows startup separately if desired.

The database is local, not encrypted. It and the journals can contain personal paths and app arguments. Protect them with your Windows account permissions and do not attach them to public issues. Public documentation screenshots use fictional data. Release packages contain no usage database, local shortcut inventory, personal screenshots or undo backups.
