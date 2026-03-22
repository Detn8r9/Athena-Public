# Athena Micro – Code review (inefficiencies, redundancy, potential issues)

**Updates applied:** Single `extract_keywords`/`is_stopword` in `db.rs` (used from `lib.rs`); `send_message` aligned with streaming path (RRF, mentioned-doc section, same section order); RRF source-name dedup via `HashSet`; `build_persistent_knowledge_section` simplified to one trim+empty check; rebuild uses `.expect(...)` instead of `.unwrap()` on `text_content`.

## 1. Duplicate `extract_keywords` and `is_stopword` (maintainability / consistency)

**Locations:**  
- `athena/src-tauri/src/lib.rs` (approx. lines 715–748): `is_stopword`, `extract_keywords`  
- `athena/src-tauri/src/db.rs` (approx. lines 9–42): same logic

**Issue:**  
Identical stopword list and keyword extraction logic exist in both files. Changing one and not the other risks inconsistent RAG behavior and bugs.

**Recommendation:**  
Keep a single implementation (e.g. in `db.rs`), re-export it, and use it from `lib.rs` so both keyword search and context-building use the same behavior.

---

## 2. Non-streaming `send_message` is out of date (inconsistency / dead path)

**Location:**  
`athena/src-tauri/src/lib.rs` (approx. lines 1099–1170): `send_message` command

**Issue:**  
- Frontend only calls `send_message_stream` (see `App.tsx` around 1738 and 1911).  
- `send_message` still uses the old retrieval: 10 keyword docs, 15 semantic chunks, no RRF, no `chat_id`, no context summary, no “mentioned document” section, no conversation-context prompt.  
- If anything (tests, future code, or a fallback) ever calls `send_message`, it will get weaker, inconsistent behavior compared to the streaming path.

**Recommendation:**  
Either (a) align `send_message` with the streaming path (same retrieval, context summary, RRF, mentioned-doc section, conversation context), or (b) document that only `send_message_stream` is supported and deprecate/remove `send_message`.

---

## 3. RRF source-name deduplication is O(n) per path

**Location:**  
`athena/src-tauri/src/lib.rs` – `build_context_rrf` (approx. line 938):  
`if !source_names.contains(&name) { source_names.push(name); }`

**Issue:**  
`source_names` is a `Vec`; `.contains()` is O(n) per insertion. With many paths this is unnecessarily slow.

**Recommendation:**  
Use a `HashSet<String>` for “already seen” source names (or track by path), then collect into the final `Vec<String>` for citations. O(1) per check.

---

## 4. `search_similar_chunks` scalability (no vector index)

**Location:**  
`athena/src-tauri/src/db.rs`: `search_similar_chunks` loads all rows from `chunk_embeddings`, computes cosine similarity for each, then sorts and takes top_k.

**Issue:**  
For large libraries this is a memory and CPU bottleneck. No HNSW or other vector index is used.

**Recommendation:**  
Document this as a known limitation for large libraries. Optionally plan a later change to use a vector index (e.g. sqlite-vec, or an external index) when the library grows.

---

## 5. `build_persistent_knowledge_section` – redundant empty check

**Location:**  
`athena/src-tauri/src/lib.rs` (approx. 1017–1026):  
First `content.is_empty()`, then in the `else` block `trimmed.is_empty()`.

**Issue:**  
When `content` is empty, `trimmed` is too; the first check makes the second redundant for that case. Logic can be simplified to a single trim-then-check.

**Recommendation:**  
Use `let trimmed = content.trim();` and a single `if trimmed.is_empty() { return String::new(); }` (and use `trimmed` for the rest). Clearer and one fewer branch.

---

## 6. Rebuild vector index – use of `.unwrap()` on `text_content`

**Location:**  
`athena/src-tauri/src/lib.rs` (approx. line 273):  
`let text = row.text_content.as_deref().unwrap();`

**Issue:**  
The iterator only includes rows with non-empty `text_content`, so this is currently safe. If the filter is ever relaxed, this could panic.

**Recommendation:**  
Use `.expect("filter guarantees non-null text_content")` or `if let Some(text) = row.text_content.as_deref()` and skip otherwise, to make the invariant explicit and avoid accidental panics.

---

## 7. Config read repeatedly from disk

**Location:**  
`athena/src-tauri/src/lib.rs`: `load_config(&app_data_dir)` is called in many commands (e.g. send_message, send_message_stream, list_library_contents, etc.).

**Issue:**  
Each command reads the config file from disk. For high-frequency flows this adds avoidable I/O.

**Recommendation:**  
Consider caching config in app state (e.g. with invalidation on save or a short TTL), or at least document that config is read per request. Low priority unless profiling shows config read as hot.

---

## 8. `save_chat` – duplicate detection scans all chat files

**Location:**  
`athena/src-tauri/src/chats.rs` (approx. 106–129): When `id` is `None`, the code scans all JSON files in the chats dir and compares message count and content to find an existing chat to update.

**Issue:**  
O(n) in number of saved chats; with many chats this could be slow. Content comparison is exact (byte-for-byte), so any frontend normalization (e.g. trimming) could prevent matches and create duplicates.

**Recommendation:**  
Accept for now; document the scaling. If duplicate chats become a problem, consider a small index (e.g. hash of first/last message) or storing a “content hash” when saving.

---

## 9. Frontend – citations not persisted

**Location:**  
Chat load in `App.tsx`: loaded messages are `SavedMessage[]` (role + content only). Citations (`sources`) are only in the in-session assistant message and are not saved.

**Issue:**  
Reloading a chat does not restore citation badges; this matches the current design (sources are session-only).

**Recommendation:**  
No change required unless you decide to persist sources (e.g. add `sources` to `SavedMessage` and to the save/load format).

---

## 10. `get_device_stats` – blocking sleep on async path

**Location:**  
`athena/src-tauri/src/lib.rs` (approx. 691–695):  
`std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL)` called twice for CPU usage sampling.

**Issue:**  
If this command is invoked from the UI thread, it blocks for the sleep duration. CPU usage calculation requires a delta between two samples, so the sleep is intentional but blocking.

**Recommendation:**  
Document that this command may block briefly, or run it in `spawn_blocking` so the main thread stays responsive. Low priority unless the UI feels frozen when opening a stats view.

---

## Summary of suggested actions

| Priority | Item | Action |
|----------|------|--------|
| High | Duplicate `extract_keywords` / `is_stopword` | Single implementation in `db.rs`, use from `lib.rs` |
| High | `send_message` out of date | Align with streaming path or deprecate/remove |
| Medium | RRF source dedup | Use `HashSet` for seen source names |
| Medium | `build_persistent_knowledge_section` | Single trim + empty check |
| Low | Rebuild `text_content` access | Replace `.unwrap()` with `.expect(...)` or `if let` |
| Low | `search_similar_chunks` | Document scaling limit; plan vector index later |
| Low | Config reads | Consider caching or document per-request read |
| Low | `save_chat` duplicate detection | Document O(n); consider content hash later |
| Low | `get_device_stats` | Document blocking or run in `spawn_blocking` |
