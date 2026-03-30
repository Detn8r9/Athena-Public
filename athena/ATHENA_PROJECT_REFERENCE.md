# ATHENA — complete application reference

**Autonomous Tactical Heuristic Emergency Network Archive (ATHENA)** — the product is referred to as **Athena** in the UI.

This document describes the **Athena** desktop application in full: purpose, architecture, features, settings, and behavior. It is intended for retrieval when the user prefixes a chat message with **`/athena`** so the model can answer accurately about the app itself.

**Version note:** This file ships with the application source tree. Feature names and behavior refer to the `athena` Tauri app unless stated otherwise.

---

## 1. What Athena is

- **Athena** is a **local-first**, **offline-capable** desktop assistant focused on **survival, repair, first aid, and practical field knowledge**.
- Inference runs on the user’s machine via **Ollama** (LLM). No cloud API is required for chat.
- Answers are **grounded in the user’s Library**: documents in a chosen folder are **indexed** (keyword + **vector / semantic** search) and relevant excerpts are injected into the model context (**RAG**).
- The app is built with **Tauri 2** (Rust backend + webview UI), **React** + **TypeScript** frontend, and a **SQLite** database (`athena_index.db`) for the index.

---

## 2. Slash command: `/athena`

- In the **main chat** (and map chat), if the user message **starts with** `/athena` (case-insensitive), the backend treats it as a request to prioritize **this reference** over generic library RAG.
- The prefix is **stripped** before the model sees the user turn; the rest of the line is the actual question.
- If the user sends only `/athena` with no further text, the app uses a default prompt asking for a concise overview of Athena’s capabilities from this reference.
- **Purpose:** Answer questions about **what Athena is**, **menus**, **settings**, **Meshtastic**, **Library**, **shortcuts**, **limitations**, etc., without relying on unrelated documents in the Library.

---

## 3. Core stack & data flow

| Layer | Technology |
|--------|------------|
| UI | React 19, Vite, CSS |
| Shell | Tauri 2, WebView |
| Backend | Rust (`src-tauri`) |
| LLM | Ollama (local HTTP, default port 11434) |
| Embeddings | Local embedding model via `embeddings` module |
| Index | SQLite: files, chunks, vectors; `vector_index` for (re)building embeddings |

**Chat flow (streaming):** Frontend calls `send_message_stream` with `message`, optional `history`, optional `chat_id`, optional GPS. Backend builds a **retrieval query** (may include recent conversation for follow-ups), runs **keyword + semantic** search (**RRF** fusion), assembles **system prompt** (rules + library excerpts + location + maps/POIs + personality + persistent `knowledge_block.txt`), then **streams** tokens via `athena:stream-chunk` / `athena:stream-done` / `athena:stream-error`. Sources may be emitted with `athena:stream-sources`.

---

## 4. Library (documents & RAG)

- User sets a **Library folder** in **Settings → Library** (or the Library modal). The **file watcher** indexes new/changed files.
- **Supported content** includes PDFs, text, images (with OCR where applicable), and **Kiwix `.zim`** archives (articles extracted and indexed).
- **Vector index:** Chunked text, embeddings stored in DB; **Rebuild vector index** (Library) recomputes embeddings for all indexed documents (use after bulk adds).
- **Learned notes:** Background process can distill “learned” notes from text for retrieval (`learner` module).
- **`knowledge_block.txt`** in app data: **persistent** user-editable block injected into **every** query (survival facts, preferences).
- **Quarantine:** Unsafe or unreadable files may be quarantined with a user-visible notification.

---

## 5. UI overview (main window)

### 5.1 Sidebar (chats)

- List of **saved chats**; **+** new chat, **temp** temporary session.
- **Pin**, **delete**, **search**; collapse control.
- **Mesh-originated** chats (from Meshtastic auto-reply) appear as separate chats, often titled like **Athena (private) · …**.

### 5.2 Header icons (typical order)

- **Export PDF** — export current conversation.
- **Emergency triage** — menu of **first aid / emergency** checklists (full-screen overlays with steps, Do / Do NOT).
- **Library** — folder, file list, **Download more data** (Kiwix), **Scan for Local Files**, **vector rebuild**.
- **Meshtastic** — USB mesh radio panel (Primary channel broadcast, received log; optional ACK; help under **?**).
- **Settings** — model, features, UI, (hidden developer after 7 taps on title).
- **Help** — structured help modal with categories.
- **Battery** (when available) — OS-reported battery pill.

### 5.3 Composer

- **Tools** — attach images, attach files, optional speech (when enabled).
- **Send**; **Enter** sends, **Shift+Enter** newline.
- Shortcuts: **⌘/Ctrl+K** or **/** focus input; **⌘/Ctrl+F** find in chat.
- Special client command **`/rag-count`** (handled in UI) shows RAG document/chunk counts.

### 5.4 Map (when enabled)

- **Features → Location-aware answers** enables map-related behavior; POIs, offline **map downloads**, **Leaflet** map UI, optional map-side chat with `send_message_stream` and GPS when relevant.

---

## 6. Settings (backend config: `athena_config.json`)

The UI groups categories as **General** (help, shortcuts, optional device overlay), **AI & models** (Ollama, tier, memory, citations), **Location & maps**, **Mesh & voice**, **Appearance & theme**, **Troubleshooting** (error log, developer access hint).

| Area | Options |
|------|---------|
| **AI & models** | Model tier (auto or override), **memory_messages** (context window size), hardware estimate, **show sources in chat** |
| **Location** | **Location-aware answers** + address (text) |
| **Mesh & voice** | **Meshtastic auto-reply**; **Text-to-speech** + voice presets (when exposed in UI) |
| **Appearance** | **Fullscreen**; **themes** (presets + custom themes: colors, fonts, personality); **night vision** modes |
| **Troubleshooting** | **Error log** copy; hidden **Developer** panel (tap **Settings** title ×7 on main menu) |

---

## 7. Meshtastic (USB mesh radio)

- **Serial listener** (`mesh_listener`) decodes **plain text** packets; **encrypted** packets are ignored for display.
- **Mesh panel UI:** Primary channel (**channel 0**) **broadcast** only in the **Received** log (public mesh traffic).
- **Send:** Broadcasts on Primary; optional **ACK**.
- **Auto-reply (`mesh_ai`):** If enabled and Ollama is running, inbound **Primary** traffic can trigger **Ollama**; replies are sent as **DMs** to the sender; chats saved as **`mesh_<node_id>`** / titled **Athena (private) · …**.
- **Role:** User’s radio is typically **CLIENT** when used as a desktop USB station (see Meshtastic docs).

---

## 8. Ollama integration

- App checks **Ollama** availability; may **attempt to launch** Ollama on startup (platform-specific paths).
- **Model selection** uses **tier** (small/medium/large) vs hardware **recommendations** (`system` module).
- If Ollama is not running, the user sees a **banner** and chat send fails until Ollama is started and a model is installed.

---

## 9. Tauri commands (representative)

Not exhaustive; see `src-tauri/src/lib.rs` `invoke_handler`:

- `send_message`, `send_message_stream`
- `set_watch_folder`, `get_watch_folder`, `list_library_contents`, `ingest_files`, `remove_library_file`, `rebuild_vector_index`, `get_rag_stats`, `get_index_stats`
- `get_system_specs`, `get_memory_estimate`, `get_model_override`, `set_model_override`, `get_memory_messages`, `set_memory_messages`
- `get_feature_map_enabled`, `set_feature_map_enabled`, `get_user_location`, `set_user_location`
- `get_show_sources_in_chat`, `set_show_sources_in_chat`
- `get_mesh_ai_auto_reply`, `set_mesh_ai_auto_reply`, `send_mesh_message`
- `get_theme_config`, `set_theme_config`, `set_fullscreen`, `is_fullscreen`
- `scan_kiwix_zim_files`, `download_map`, `get_map_catalog`, `get_downloaded_maps`, `get_smallest_kiwix_map_suggestion`, `check_network_reachable`, `geocode_place_query`, `list_tile_packs_cmd`, `offline_basemap_paths_cmd`, `remove_tile_pack_cmd`, `start_tile_pack_download`, `get_pois`, `add_poi`, `remove_poi`, `update_poi`
- `get_local_time`, `get_sun_times`, `get_battery_state`, device stats, etc.

---

## 10. Privacy & safety

- **Local:** Chat storage, index, config live under the OS **app data** directory (Tauri `app_data_dir`).
- **No cloud LLM** in the default path; only local Ollama.
- **Location** text and GPS (when used) are for **local** context; **no** live cloud lookup for “what’s open now.”
- **Emergency triage** is **educational**; not a substitute for professional care or emergency services.

---

## 11. Platform notes

- **Windows / Linux / macOS** supported; some **device overlay** metrics (battery, GPU temp) vary by OS — see `docs/product/PLATFORM_COMPATIBILITY.md`.
- **macOS** fullscreen uses native fullscreen; **Escape** exits.

---

## 12. Limitations (honest)

- **Single USB radio** per Athena instance: typical Meshtastic constraint (often **USB OR BLE** to phone, not both simultaneously).
- **Mesh:** No built-in discovery of “other Athena instances” or cross-PC chat sync; each install is **standalone** unless extended.
- **RAG** quality depends on **what’s in the Library** and **chunking**; very large libraries need rebuilds after bulk changes.

---

## 13. Related files in repo

*(Paths below are from the **repository root**, i.e. the parent folder of `athena/`.)*

- `athena/src/HelpModal.tsx` — in-app help (categories).
- `athena/src/MeshRadioModal.tsx` — mesh UI.
- `docs/development/OPTIMIZATION_AUDIT.md`, `docs/product/PLATFORM_COMPATIBILITY.md`, `docs/development/CODE_REVIEW.md` — engineering notes.

---

*End of Athena application reference.*
