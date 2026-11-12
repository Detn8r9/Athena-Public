# Full feature test: 12 November 2026

Branch `polish/rag-and-reliability`, debug build (`npm run tauri dev`) on macOS, test data root
`~/AthenaTestRoot`. The app was driven through the debug-only dev bridge (`ATHENA_DEV_BRIDGE=1`,
127.0.0.1:8799) from the built-in browser at `http://localhost:1420/?devbridge=1`, because
desktop screen control was not granted. Native-window behaviour (tray, OS dialogs, file pickers)
was therefore not exercised.

Final suites: `cargo test` 200 passed / 4 ignored, `vitest` 83 passed, `npm run build` OK,
`cargo check --release` OK.

## Fixed during this pass

| Area | Problem | Fix |
|---|---|---|
| Whole app | Window froze (black lock screen) while books were indexing: the database lock was held during embedding. | Embedding now runs without the lock; it is taken only for the write. Other commands answer in about 0.4 s during indexing. |
| Whole app | Slow commands (chat, titles, library build, help, logistics) ran on the main thread and froze the window. | Made async, run on a worker thread. |
| Whole app | Crash at startup: Watch Officer read app state before it existed. | Waits for state to be ready. |
| Library / Academy | App aborted when opening modern ZIM files (overflow panic in zim 0.4). | Upgraded to zim 0.5, opening is guarded; all 18 test ZIMs open. |
| Academy | Lessons never loaded: wrong ZIM picked (non-English), wrong namespace, wrong data folder in portable mode. | Ranks English ZIMs matching the hint, tries both namespaces, follows redirects, clear error message. |
| Academy | Kiwix panel showed a broken frame when kiwix-serve was not running. | Checks reachability, shows a message and a "Check again" button. |
| Chat | Answers cited unrelated sources ("Group memory: North well" in a question about another topic): short complete questions were treated as follow-ups. | Follow-up detection now needs a referring word or no real content words. |
| Chat | Group memory files showed as raw paths in sources. | Shown as "Group memory: <first line>". |
| Library | Large books were cut at 1 MB of text, and trimmed books lost page numbers. | Cap raised to 8 MB; page map kept when trimming. |
| Logistics | Every AI note fell back to "General, no inventory" (field name mismatch). | Accepts both names; parser tested. |
| Maps | Map assistant showed nothing when it ran tools without text, and gave no help when it could do nothing. | Shows a summary of what it did, and explains how to add a marker. |
| Portable mode | Map region extract, tile packs, satellite download, storage and the tile protocol wrote to the system data folder. | Use the portable data folder. |
| Catalog | Survivor Library entry (removed from archive.org), stale Wikipedia file, placeholder satellite entry, wrong sizes. | Replaced with Appropedia, current file, entry removed, sizes from the servers. |

## Checked and working

Sensors (low reading flagged on Dashboard), radio silence (manual and overnight windows), modem
write/read round trip (damaged recording refused), intercept watch words, courier export/import
(13 records, re-merge changes nothing, signature verified, tampered copy refused), dead reckoning
with declination, downwind plot, paper binder (6 sections, no patient or key data), Comms Hub,
library listing, chat citing Hesperian books, map assistant summary, logistics note to proposal
to inventory (about 10 s), Academy ZIM list and lessons (0.2–0.4 s).

## Open issues

| # | Area | Issue | Severity |
|---|---|---|---|
| 1 | Field Ops | Inputs have no accessible labels; Add buttons show no busy state. | Medium |
| 2 | Field Ops | "Water jug runs out in 2 days" shown next to "water lasts 12 days". | Medium |
| 3 | Maps | Duplicate "Map chat…" bar; focusing the assistant scrolls the controls. | Low |
| 4 | Maps | Sidebar labels truncated ("Directi…", "Territor…"). | Low |
| 5 | Maps | The map assistant has no add-marker tool. | Medium |
| 6 | Inventory | Queue popover background is see-through. | Low |
| 7 | Library | Page has no heading. | Low |
| 8 | Chat | Shows a raw "Not authenticated" when the session expires instead of returning to the lock screen. | Medium |
| 9 | Chat / Logistics | Small local model gives wrong facts and missed "Used 2 bandages". | Medium |
| 10 | Library | ZIMs copied in by hand are served by kiwix-serve only after a restart. | Medium |
| 11 | Library | Writes are slow while a large library is indexing. | Low |
| 12 | Library | Book chunks indexed before the page fix have no page numbers until re-indexed. | Low |
| 13 | Personnel | Shows 3 Level 5 operators (caused by the test account; check on a clean install). | Low |

## Not built yet

S1 per-user stream isolation, S7 broadcast, S8 map fading, S14 dictation, S16 tier picker,
S17 camera lookup, S27 journal and children's curriculum slot. Content for S16 and S27 is listed
in `docs/LIBRARY_CONTENT.md`.


## Follow-up the same day: fixes and Library Vault

Checked in the running app (dev build, test data folder) and by automated tests: 245 backend,
86 frontend and 6 relay tests pass.

| # | Status | What changed |
|---|---|---|
| 1 | Fixed, not checked by screen reader | Field Ops inputs get labels; screens show "Saving…" while busy. |
| 2 | Fixed | Item rows say they are each item on its own; the needs line says "All water together lasts…". |
| 3 | Fixed, checked | One map assistant bar. |
| 4 | Fixed, checked | Map labels read "Route" and "Areas" and wrap instead of cutting off. |
| 5 | Fixed, checked | "Add a marker called Water cache" offers an Add button and saves at the map centre (category Water). |
| 6 | Fixed, checked | Queue popover is solid. It still clips under the header when the page is scrolled. |
| 7 | Fixed, checked | Library page heading. |
| 8 | Fixed | An expired session returns to the lock screen with "Your session ended". |
| 9 | Fixed, checked | Simple notes such as "Used 2 bandages" are read without the model (−2 Bandages, medical). The new-item prompt no longer offers to add stock at −2. |
| 10 | Fixed | A ZIM copied in by hand restarts the reader without an app restart. |
| 11 | Fixed | Index writes go in batches of 200 and release the database between them. |
| 12 | Fixed | Books whose passages lack page numbers are re-indexed. |
| 13 | Not changed | Caused by the test account. |

Library Vault (Library → Vault), checked in the app:

- Coverage counts indexed files, files still waiting on a shelf, and every ZIM. It first showed only
  4 books because it read the index alone; that is fixed.
- Pack of the Skills shelf: 1.3 MB became 407 KB, checksums matched, and chat search added 1,469
  passages.
- Device scan on a sample folder: a vehicle manual and a solar note were copied and marked as
  filling gaps. A one-line supply-cache note went to the private shelf; at first it was missed
  because files under 512 bytes were skipped, and that limit is now 64 bytes. A key file and a
  shopping list were skipped.
- Update check found a newer NHS medicines release.
- Alerts, hunker-down, Other stations and settings render. With alerts off and no other station
  running, the feeds and the mesh search were not tried from the screen; their tests pass.
