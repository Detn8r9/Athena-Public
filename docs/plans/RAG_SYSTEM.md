# Athena Library RAG — Current Implementation

This document describes how **Retrieval-Augmented Generation** for the local Library works in the Tauri backend as of the codebase that includes `indexer.rs`, `vector_index.rs`, `chunking.rs`, `embeddings.rs`, `db.rs`, and `lib.rs` (chat streaming).

---

## 1. Purpose

RAG augments chat by pulling **excerpts from locally indexed Library files** (plus a few special types) and injecting them into the prompt as **library context**. It does **not** call an external embedding API: everything is **offline** and stored in **SQLite** (plus **sqlite-vec** for vector search).

---

## 2. Ingestion: what gets indexed

**Trigger:** The file watcher runs on configured watch roots (`watcher.rs`). On create/modify it calls `indexer::index_file`; on delete it removes the row and chunk embeddings for that path.

**Per file, the indexer:**

- Classifies the file (text, PDF, Pages, Office formats, image, etc.).
- Extracts **one full-document string** where possible (e.g. plain text read, `pdf_extract` + optional OCR merge for PDFs, Pages bundle logic, docx/odt zip XML, image OCR, etc.). That string is **trimmed and capped** (`MAX_INDEXED_TEXT_BYTES`, 1M bytes in `indexer.rs`).
- Writes a row into **`indexed_files`** with `file_type`, `name`, `path`, and **`text_content`** (the full extracted text for searchable types).

**Embeddings (second store):** If `text_content` is non-empty after indexing, the watcher calls `vector_index::index_document`, which:

1. Deletes existing rows for that `file_path` in **`chunk_embeddings`** / **`chunk_embeddings_vec`**.
2. Chunks the full text with `chunking::chunk_text_default` (semantic chunking with defaults: ~800 target / 1200 max chars per chunk, then word-limit pass with overlap).
3. For each chunk, builds a vector with `embeddings::embed_text` and upserts via `db.upsert_chunk_embedding`.

**Two parallel representations:**

| Store | Role |
|--------|------|
| **`indexed_files.text_content`** | Full document text; used for **keyword / LIKE** retrieval and for **building snippets** when a doc is selected by keyword RRF. |
| **`chunk_embeddings` (+ vec index)** | **Chunk text + 256-dim vector** per chunk; used for **similarity search**. |

If extraction yields **no** text, nothing is embedded (no chunks), and keyword search will not match that file unless another mechanism applies (e.g. recent-doc fallback).

---

## 3. Embeddings (what “semantic” actually means)

`embeddings.rs` defines **deterministic, local** vectors: tokenize (alphanumeric, min token length 2), hash each token into one of **256 buckets**, accumulate, **L2-normalize**. This is **not** a neural embedding model; it is a **bag-of-hashed-tokens** representation. Similarity search is **cosine-style** on those vectors (sqlite-vec KNN with a scan fallback in `db.rs`).

---

## 4. Retrieval query (what you search with)

**Streaming main chat** (`lib.rs`, chat stream path):

- Builds a **`retrieval_query`** string. For a loaded chat with a **context summary**, it uses `"{summary} {current message}"` so follow-ups stay on topic; otherwise it uses the current user message (and can incorporate history via `build_retrieval_from_history` / saved messages where wired).
- **`/athena` ref mode** turns off library RAG entirely for that turn (reference doc only).

---

## 5. Three ways documents enter the prompt

### A. “Mentioned by filename”

`get_indexed_files_by_name_mentioned` (`db.rs`) loads indexed files whose **`name` appears as a substring of the query** (case-insensitive), and only if they have non-empty `text_content`. Those docs are formatted with `build_context_from_docs` (up to 4000 chars budget for that block) and prefixed with instructions to use them—e.g. user says “in House Details.pdf”.

### B. Keyword search on full text (`search_text_content`)

- Takes up to **20 keywords** from the query via `extract_keywords` (stopwords stripped, length ≥ 3, first 20 tokens).
- SQL: `text_content LIKE '%keyword%'` with **OR** across keywords; `file_type` in `text`, `zim`, `learned`, `image` (images can have OCR text).
- Ordered by **count of keyword hits**, then recency.
- If **no** keywords survive (all stopwords, etc.), it falls back to **most recent** text files up to the same limit.

Main chat uses limit **20** normally, **30** when `retrieval_query` includes conversation context (summary + message ≠ bare message).

**Learned** rows are sorted first (`sort_docs_learned_first`) so compressed Q/A-style notes rank ahead of ordinary files when both match.

### C. Semantic search on chunks (`search_similar_chunks`)

- Embeds **`retrieval_query`** with the same `embed_text`.
- Retrieves **top_k** chunks (25 default, **12** when conversation context is on).
- Returns chunk rows including **`chunk_text`** (what the model actually sees for chunk-only paths).

---

## 6. Merging keyword docs + chunks: RRF

`build_context_rrf` (`lib.rs`) implements **Reciprocal Rank Fusion**:

- Builds a map **keyed by file path**.
- Each **keyword-hit document** adds score `1 / (60 + rank)` (rank starts at 1). Constant `RRF_K = 60`.
- Each **semantic chunk** adds the same score to its **`file_path`** (chunks accumulate under one path).
- Paths are sorted with **keyword hits preferred** over semantic-only, then by fused score.
- Takes the top **15 paths** (`RRF_TOP_N`).
- For each path, allocates a **per-path character budget** so total injected text stays around **`RRF_COMBINED_MAX_CHARS` (12_000)**.

**Per path, context is built as:**

- If there is a **keyword `IndexedFile`**: use **`build_context_from_docs`** on the **full `text_content`** (snippet windows / full small docs—keywords from query use up to 6 terms in that function, with windows and `FULL_DOC_THRESHOLD` 7000).
- Else if there are **chunks only**: use **`build_semantic_context`** (concatenates chunk texts until budget; skips empty chunk bodies).

**Follow-up mode:** When `has_conversation_context` is true, `build_context_rrf` is called with **`keyword_only_for_followup = true`**, so **semantic-only paths are dropped** unless that path **also** appeared in the keyword doc list. That keeps RAG from drifting on “it / that” follow-ups.

**Sources list:** Display names are collected only when a path **actually contributed non-empty** context text.

---

## 7. Fallback when RRF context is empty

If fused RRF text is empty, the code pulls **3 most recent** text files and uses `build_recent_docs_context` (short snippets, 2000 char total cap) with a “recent library documents” instruction.

---

## 8. What gets injected into the model

The assembled blocks (**mentioned docs + RRF excerpts + optional recent fallback**) are concatenated into a **context section**. If that section is **empty**, the system uses a **different** library rule (tell the user to add documents). If non-empty, it uses **`LIBRARY_RULE_WITH_CONTEXT`** (ground answers in excerpts, don’t invent property details, etc.).

**Sources** emitted to the UI are: mentioned doc names plus RRF `source_names` (capped at 12), merged without duplicates.

---

## 9. Chat console vs main chat

- **Main streaming chat:** Full flow above (summary-aware `retrieval_query`, stricter RRF when context exists, sources emission).
- **Console chat** (`console_chat_library_context` and related path in `lib.rs`): Same **RRF + keyword + semantic** idea, but **no** conversation-summary merge in the console helper (single-turn query string); the unified console command still uses `console_chat_library_context` before the model call.

---

## 10. Key source files

| Area | Location |
|------|-----------|
| File watcher + `vector_index::index_document` | `athena/src-tauri/src/watcher.rs` |
| Extraction per format | `athena/src-tauri/src/indexer.rs` |
| Chunking | `athena/src-tauri/src/chunking.rs` |
| Embeddings | `athena/src-tauri/src/embeddings.rs` |
| Chunk store + search | `athena/src-tauri/src/db.rs`, `athena/src-tauri/src/vector_index.rs` |
| RRF, prompt assembly, streaming chat | `athena/src-tauri/src/lib.rs` |
| Console RAG wrapper | `athena/src-tauri/src/commands/chat_console.rs` (`console_chat_library_context`) |

---

## 11. Limitations (behavioral)

- **“Semantic”** is **hash-token similarity**, not transformer embeddings; quality is coarser than cloud RAG.
- **Keyword** search is **substring LIKE**, not full-text stemming / inverted index.
- **No text extracted ⇒ no row match and no chunks** for that file (except filename mention if the name appears in the query and `text_content` is non-empty).
- **ZIM** and other types follow the same pattern where implemented; unsupported types may not contribute text.

---

## 12. End-to-end summary

**Watched file → `indexed_files` + `chunk_embeddings` → retrieval query → keyword docs + semantic chunks → RRF merge → snippet building → library context + sources in prompt.**
