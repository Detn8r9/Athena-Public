# Phase 0 — Baseline audit (reference)

This note captures pre-integration behavior for RAG, maps, and Kiwix. Use it to compare after sqlite-vec and other phases.

## RAG / vector search

- **Embeddings:** Deterministic 256-dim hash vectors in [`src-tauri/src/embeddings.rs`](../src-tauri/src/embeddings.rs) (`EMBEDDING_DIM = 256`). No external model.
- **Storage:** Table `chunk_embeddings` in SQLite (`vector` BLOB as packed `f32`).
- **Query (updated):** [`Database::search_similar_chunks`](../src-tauri/src/db.rs) uses **sqlite-vec** (`chunk_embeddings_vec` virtual table) for kNN when available, then joins back to `chunk_embeddings`. If the vec table is missing or the query fails, it falls back to the legacy path: up to **25,000** rows by `id` order, **cosine** in Rust, top-k.
- **Rebuild:** `rebuild_vector_index` re-embeds library files via [`vector_index::index_document`](../src-tauri/src/vector_index.rs).

### Suggested metrics to log (optional)

- Chunk count: `SELECT COUNT(*) FROM chunk_embeddings`.
- Time `search_similar_chunks` for a typical query (add `std::time::Instant` in dev builds if needed).

## Maps / tactical

- POIs and tactical layers via Tauri commands; Leaflet in React. Baseline: no offline GraphHopper/OSRM in-app (Phase 4 adds routing).

## Kiwix

- ZIM discovery, download, and scan in [`src-tauri/src/kiwix.rs`](../src-tauri/src/kiwix.rs). Optional `kiwix-search` CLI: [`kiwix_search_with_tool`](../src-tauri/src/lib.rs) + [`kiwix_tools.rs`](../src-tauri/src/kiwix_tools.rs) when `kiwix_search_binary_path` is set in config (GPL-3 tools — see `THIRD_PARTY_NOTICES.md`).

## Config

- Single JSON [`athena_config.json`](../src-tauri/src/config.rs) in app data; no version field yet (cross-cutting migration todo).
