# Protect matching local lyrics during optional desktop cleanup — 2026-11-09

## Reproducible problem
The existing Desktop Pilot classifier offers MP3/WAV/FLAC/OGG/M4A as Media and TXT as Documents; LRC is not offered. Its sidecar protections did not recognize same-name external lyrics. The opt-in organizer could move Album.flac and Album.txt into different category folders, or Song.mp3 into Media while leaving Song.lrc on Desktop. This is a source-confirmed risk, not a reported user data-loss event.

## Primary documentation
Plex support, Adding Local Lyrics (https://support.plex.tv/articles/215916117-adding-local-lyrics/), requires matching audio and external timed .lrc or plain .txt lyrics in the same directory. The official Kodi CU LRC Lyrics add-on (https://kodi.tv/addons/omega/script.cu.lrclyrics/) likewise uses matching local lyrics. Gajos et al., CHI 2008 (https://www.eecs.harvard.edu/~kgajos/papers/2008/gajos08predictability.shtml), motivates predictable and accurate adaptation, not a particular filename heuristic.

## Minimal correction
The existing bounded non-recursive Desktop snapshot now protects exact-stem audio/.lrc and audio/.txt pairs, including the eligible .txt side. No file contents are read, no new settings or telemetry are introduced, and unrelated files are not moved differently. Coincidental same-stem audio and notes may be kept conservatively. The same dependency scan runs both in Analyze and Apply.

## Production regressions
- Real Windows/SQLite organizer Plan, Apply, Undo: two audio/lyrics pairs stay byte-identical; two independent Media plus two Documents move and undo. Saved manual Always rule and history are unchanged; no journal remains.
- A late .lrc or .txt file after preview invalidates the old plan before any move or new folder.
- Exact matching and the existing 6000-item bound are exercised with Unicode and literal percent names.

Do not interpret synthetic fixtures as measured human time savings. Run the full exact-head Windows 2022/2025/11 ARM CI, native Explorer/Cloud Files and interface checks before merging into organizer integration. No stable main release or 0.3.0 publication without the already documented physical acceptance gates.
