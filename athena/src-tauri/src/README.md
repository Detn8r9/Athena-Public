# Rust backend (`src-tauri/src/`)

Tauri commands and core logic. **`lib.rs`** wires modules, plugins, and `invoke` handlers; **`main.rs`** is the binary entry.

## Module map (file → responsibility)

| File | Area |
|------|------|
| **`lib.rs`** | App setup, command registration, shared state |
| **`main.rs`** | Entry point |
| **`config.rs`** | App configuration load/save |
| **`db.rs`** | SQLite / persistence |
| **`chats.rs`** | Chat sessions and messages |
| **`indexer.rs`** | Library file indexing |
| **`chunking.rs`** | Text chunking for RAG |
| **`embeddings.rs`** | Embedding pipeline |
| **`vector_index.rs`** | Vector search index |
| **`watcher.rs`** | Library folder watching |
| **`quarantine.rs`** | Unsafe / blocked file handling |
| **`kiwix.rs`** | Kiwix ZIM discovery & related |
| **`maps.rs`** | Map ZIM catalog, downloads |
| **`map_manager_catalog.rs`** | Offline Map Manager catalog (embedded JSON; `get_offline_map_manager_catalog`) |
| **`tile_pack.rs`** | Offline raster tile packs |
| **`mbtiles_io.rs`** | MBTiles I/O |
| **`pmtiles_io.rs`** | PMTiles I/O |
| **`ocr.rs`** | OCR |
| **`tts.rs`** | Text-to-speech |
| **`ollama.rs`** | Ollama / LLM integration |
| **`learner.rs`** | Learning / memory helpers |
| **`theme.rs`** | Theme persistence |
| **`system.rs`** | System info, device metrics |
| **`mesh_radio.rs`** | Mesh radio (Meshtastic) |
| **`mesh_listener.rs`** | Mesh listener / ingest |
| **`mesh_ai.rs`** | Mesh + AI bridge |

## Crates & assets

- **`../Cargo.toml`** — Dependencies and crate name
- **`../capabilities/default.json`** — Tauri 2 permissions
- **`../../ATHENA_PROJECT_REFERENCE.md`** — Included at compile time for `/athena` command (`include_str!` in `lib.rs`)
