# ATHENA — Improvement Proposals, Round 2

**Written:** 2026-11-10. These add to groups A–G that you already approved. Mark each item **approve** or **deny**; nothing here has been started.

**How to read it**

- **Evidence** says what the suggestion is based on. "Seen in code" means I read the code that shows it. "Idea" means it is a product suggestion with nothing in the code to contradict it; I checked that the feature does not already exist by searching the source, which can miss things named differently.
- **Size** is a rough effort guess: S (under a day), M (a few days), L (a week or more).
- I have not profiled the running app, so the speed gains below are expected from the code, not measured.

My top five, in order: **H1, H2, I1, J1, H4**.

---

## H. Speed and efficiency

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| H1 | **Stop indexing from freezing the app.** While a document is being embedded, the database lock is held for the whole job, so chat, the map and every other screen that touches the database wait. Compute embeddings first, then take the lock only to write. | Seen in code: `index_document_spawn_blocking` locks the database, then embeds every chunk inside the lock. | M |
| H2 | **Batch embeddings.** Chunks are embedded one at a time. Sending them to the model in batches of 16–32 should make indexing several times faster. | Seen in code: one inference call per chunk; no batching or thread settings. | M |
| H3 | **Separate read connections for the database.** The whole app shares one connection behind two nested locks, so a slow search blocks an unrelated write. WAL mode is already on, which allows many readers at once. | Seen in code: `Arc<Mutex<Database>>` wrapping `Mutex<Connection>`. | L |
| H4 | **Keep the chat model loaded.** No keep-alive is sent, so the model unloads after a few idle minutes and the next reply starts slowly. Add a keep-alive setting and warm the model when you unlock the app. | Seen in code: no `keep_alive` anywhere in the AI module. | S |
| H5 | **Smoother chat streaming.** In typed chat every token copies the message list and re-renders every message, including re-parsing Markdown. Batch updates per animation frame and memoize finished messages. | Seen in code: per-chunk `setMessages` with a full array copy; message rows are not memoized. | M |
| H6 | **Virtualize long lists.** Chat history, mesh log, inventory, personnel and library lists render every row. Nothing in the app uses list virtualization. | Seen in code: no virtual list library in use. | M |
| H7 | **Faster startup on large libraries.** A full database integrity check runs on every launch. Run a quick check at launch and the full check weekly or on demand. | Seen in code: `PRAGMA integrity_check` in `Database::new`. | S |
| H8 | **Smaller, faster first load.** The main bundle is 655 KB and the map bundle is 1 MB. Load the map, PDF export and settings code only when opened. | Measured from the production build output. | M |
| H9 | **Remove debug output from the hot path.** Every embedding call prints tensor shapes to the terminal. | Seen in code: `neural_embed.rs` line 188. | S |
| H10 | **Consolidate polling.** 25 separate timers poll for status (9 of them in `App.tsx`), some every 1.4–2 seconds. Replace with backend events or one shared status tick, and pause when the window is hidden. This matters on battery. | Seen in code: `setInterval` count. | M |
| H11 | **Cache repeated retrieval work.** Cache the query embedding and reranker scores for the last few questions so "regenerate" and follow-ups are instant. | Idea, follows from the new retrieval path. | S |
| H12 | **Use the GPU for embeddings and reranking on Apple Silicon.** Both models run on CPU only. | Seen in code: CPU execution provider only. | M |

## I. Security and data safety

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| I1 | **Harden PIN storage.** PINs are six digits stored as an unsalted SHA-256 hash, which can be reversed by trying all one million values in under a second. Move to a salted, slow hash (Argon2) and add lockout after repeated failures. | Seen in code: `personnel_pin_hash`; the comment there already says "prefer Argon2". | M |
| I2 | **Protect the LAN web host.** It serves over plain HTTP with open cross-origin access, so PINs and chat cross the Wi-Fi unencrypted. Add self-signed HTTPS with a pairing code, and restrict cross-origin access. | Seen in code: `lan_bridge.rs` uses `CorsLayer` with `Any`; no TLS. | L |
| I3 | **Encrypt data at rest.** Offer an encrypted database and chat store unlocked at login, so a lost drive or USB clone does not expose everything. | Idea. I did not find database encryption, but did not check the export path fully. | L |
| I4 | **Tighten the content security policy.** Scripts are allowed inline, which weakens protection against a malicious document or web clip. | Seen in config: `script-src 'self' 'unsafe-inline'`. | M |
| I5 | **Audit trail viewer.** A readable log of who logged in, exported, changed inventory or approved AI changes. | Idea; audit-related code exists in 9 files but I did not find a viewer. | M |
| I6 | **Duress PIN and quick wipe.** A second PIN that opens a harmless guest view, and a panic action that locks and clears sensitive caches. | Idea. | M |

## J. AI and chat

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| J1 | **Let the assistant act, with approval.** Tools the model can call: search library, look up inventory, add a logbook entry, drop a map marker, create a calendar event, send a mesh message. Each action shows a confirm card. The approval inbox you already have fits this. | Idea, builds on the existing staged-changes system. | L |
| J2 | **Regenerate, edit-and-resend, and branch.** Standard chat controls that are missing or partial. | Seen in code: "regenerate" appears in one file only. | M |
| J3 | **Chat folders, tags and full-text search across all chats.** | Not found in code. | M |
| J4 | **Saved prompts and playbooks.** One-click prompts such as "Summarize this document", "Make a checklist from this", "Draft a radio message". | Not found in code. | S |
| J5 | **"Ask this document" mode.** Open a document and chat with only that document, with the cited passage highlighted beside the answer. | Idea, builds on the new retrieval path. | M |
| J6 | **Whole-document summaries at index time.** Store a short summary per document and per section so "summarize X" and broad questions work well on small models. | Idea; learned notes are a partial version of this. | M |
| J7 | **Answer confidence.** Show whether a reply came from your library, general knowledge, or a mix, and warn when the model's context was nearly full. | Idea, the data now exists in the backend. | S |
| J8 | **Model advisor.** Recommend a model for this machine's RAM and show expected speed before download. Warn when the selected model is too large. | Partial: tier recommendation exists; no speed estimate. | M |
| J9 | **Image and scanned-page understanding in the library.** Run OCR automatically when a PDF has no text, and caption images so they are searchable. | Partial: OCR module exists with limited UI. | M |

## K. Field and survival tools

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| K1 | **Inventory alerts.** Low-stock thresholds and expiry warnings on the dashboard, with a "what expires in 30 days" view. | Low-stock logic not found; expiry fields exist in 4 files. | M |
| K2 | **Barcode and QR scanning** for inventory intake using the camera, plus printable QR labels for bins and locations. | Not found in code. | M |
| K3 | **Consumption planner.** Days of food, water and fuel remaining for N people, updated from inventory. | Idea; ration-related terms appear widely, so check for overlap first. | M |
| K4 | **Offline toolkit.** Unit converter, coordinate converter (lat/lon, MGRS, UTM), Morse and phonetic alphabet reference, sun and moon times, dosage and dilution calculators. | Converter and Morse not found in code. | M |
| K5 | **GPX and KML import/export** for routes, tracks and markers, so the map works with handheld GPS units and other apps. | Not found in code. | M |
| K6 | **Track recording and breadcrumb trail** with distance, elevation profile and "route back". | Idea. | M |
| K7 | **Medical records per person.** Allergies, blood type, medications and conditions on the personnel profile, shown on the triage screen. | Allergy and medication terms appear in only 2 files. | M |
| K8 | **Printable field packets.** One-click PDF of a checklist, map extract, contact sheet or radio plan for when the screen is not available. | Idea; PDF export exists for chats. | M |
| K9 | **Incident mode.** One switch that starts a timestamped incident log, pins triage and comms, and records every action until closed, with an after-action report. | Idea. | L |
| K10 | **Task board.** Assign tasks to personnel with due times, linked to calendar and mesh notifications. | Idea. | M |

## L. Communications (beyond group C)

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| L1 | **Store and forward.** Queue messages for nodes that are out of range and deliver when they reappear. | Not found in code. | M |
| L2 | **Position sharing and rally points.** Send your position or a map point over mesh; recipients see it on their map. | Sending not found; receiving node positions exists. | M |
| L3 | **Bluetooth and network radio connections,** not only USB. | Seen in code: serial only. | L |
| L4 | **Message compression and structured short codes** so status reports use fewer airtime bytes. | Idea. | M |
| L5 | **Radio plan generator.** Channels, schedules and call signs as a printable card, kept in sync across stations. | Idea. | M |
| L6 | **Sensor telemetry.** Show temperature, humidity and battery from Meshtastic sensor nodes, with alerts. | Not found in code. | M |

## M. Interface and experience

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| M1 | **Design system.** 841 inline style blocks make screens look slightly different from each other. Build shared components (button, card, field, table, modal, empty state) and migrate screen by screen. | Counted in code. | L |
| M2 | **Command palette for actions,** not only search: "new chat", "add marker", "send mesh message", "export". | Idea; Spotlight search exists. | M |
| M3 | **Undo for destructive actions** with a toast, instead of confirm dialogs everywhere. | Undo appears in 5 files only. | M |
| M4 | **Low-power and night modes** that dim the UI, cut animation and reduce polling. | Partial: power-save event and night-vision theme exist. | S |
| M5 | **Touch and small-screen layout** for tablets and the LAN web view. | Idea. | L |
| M6 | **Languages.** Move UI text into the existing lexicon fully so it can be translated. | Partial: a lexicon exists. | L |
| M7 | **In-app guided tours** for each workspace, replacing the long Help modal as the first stop. | Idea; Help modal is 1,085 lines. | M |

## N. Platform

| ID | Proposal | Evidence | Size |
|----|----------|----------|------|
| N1 | **Offline updates.** Apply a signed update package from a USB drive or from another station over LAN, with rollback. | No updater found. | L |
| N2 | **Station-to-station sync with conflict handling.** Choose what syncs (library, inventory, logbook, markers) and resolve conflicts visibly. | Partial: LAN and P2P sync exist; conflict handling appears in 1 file. | L |
| N3 | **Health dashboard.** One screen showing model, database, disk, radio, sidecars and last backup, each with a fix button. | Partial: resilience check exists. | M |
| N4 | **Automated release checks.** Run type-check, tests and the retrieval regression set on every commit. | Idea; a `.github` folder exists but I did not review it. | S |
| N5 | **Decide the future of `athena-web/`.** It is a second, different UI. Either retire it in favour of the LAN web host or give it a defined role. | Seen in README: marked experimental. | S |
