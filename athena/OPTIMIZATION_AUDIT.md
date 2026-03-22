# Athena – Optimization Audit (Device Demand)

This document lists inefficiencies and optimization opportunities to reduce CPU, RAM, and I/O demand. Items are grouped by area and ordered by **impact (high → low)** within each section. No code has been changed; this is analysis only.

---

## 1. Backend (Rust / Tauri)

### 1.1 Device stats: blocking main thread (~400 ms every 1.5 s when overlay on)

**Where:** `get_device_stats()` in `src-tauri/src/lib.rs`.

**Issue:** Each call does two `std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL)` (e.g. 200 ms × 2) and two `sys.refresh_cpu_usage()` on the **main thread**. Then it runs `get_cpu_temp_c()` and `get_gpu_usage_pct()` (e.g. `powermetrics` or `nvidia-smi`), which can block for another second or more. The overlay polls every 1.5 s, so the main thread is blocked for a large fraction of that time while the overlay is enabled.

**Impact:** CPU (main thread blocked), responsiveness.

**Suggestions:**

- Run `get_device_stats` (or at least the sysinfo + temp/GPU sampling) in a **spawned thread** or a small thread pool, and cache the result. Frontend polls get the last cached value; a background task updates the cache every 1–2 s.
- Optionally **increase device overlay poll interval** (e.g. 2.5–3 s) to reduce load even after moving work off the main thread.

---

### 1.2 `get_system_specs()` does a full `sys.refresh_all()`

**Where:** `get_system_specs()` in `src-tauri/src/ollama.rs` (used by `get_system_specs` command and by `get_recommended_model`).

**Issue:** `sys.refresh_all()` refreshes CPUs, memory, disks, networks, and processes. For “system specs” (RAM, core count, recommended tier) only memory and CPU list are needed.

**Impact:** CPU and I/O when opening settings or when recommended model is resolved.

**Suggestion:** Use `refresh_memory()` and a minimal CPU refresh (e.g. only what’s needed to get `cpus().len()`), or `new_with_specifics` with a narrow `RefreshKind`, instead of `refresh_all()`.

---

### 1.3 `get_indexed_files` / `db.get_all()` loads full text and metadata for every file

**Where:** `get_indexed_files` → `db.get_all()` in `src-tauri/src/lib.rs` and `db.rs`.

**Issue:** `get_all()` SELECTs `text_content` and `metadata_json` for every row. The **frontend does not call `get_indexed_files`** for the library panel (it uses `list_library_contents`, which is lightweight). So the heavy path is only used if something else (e.g. a future feature) calls it. For **search**, `search_text_content` already returns full rows including text, which is needed for context. So the main risk is any future or internal use of `get_indexed_files` on large libraries.

**Impact:** RAM and serialization when the index is large; IPC payload size if a client ever requests “all indexed files” with content.

**Suggestion:** If any caller only needs path/name/type/size, add a lighter API (e.g. `get_indexed_files_metadata()` or a `get_all()` variant that omits `text_content` and optionally `metadata_json`), and use it from the frontend or other callers that don’t need full content.

---

### 1.4 `send_message_stream` blocks main thread before spawning

**Where:** `send_message_stream()` in `src-tauri/src/lib.rs`.

**Issue:** Before `std::thread::spawn(|| chat_stream(...))`, the command does on the **main thread**: config load, `get_available_models()` (HTTP to Ollama), `get_specs()` (sysinfo), `db.search_text_content()`, and building system content and message list. For a slow disk or many files, this can add noticeable delay before the first chunk is emitted.

**Impact:** UI can feel sluggish when sending a message, especially on first send or with a large index.

**Suggestion:** Move the entire “prepare + stream” flow into a spawned thread (or async task if Tauri/async is used): load config, fetch models, get specs, run search, build messages, then call `chat_stream` in the same thread. The invoke handler only validates (e.g. Ollama running) and spawns, then returns immediately.

---

### 1.5 Watcher: initial scan blocks the watcher thread

**Where:** `run_watcher_loop` in `src-tauri/src/watcher.rs`; `initial_scan()` is called for each folder when `SetFolders` is applied.

**Issue:** `initial_scan` does a full `walkdir` and calls `indexer::index_file()` / `index_zim_file()` for every file **on the watcher thread**. Large watch folders (e.g. thousands of files or big ZIMs) can block that thread for a long time; during that time, new filesystem events are only queued.

**Impact:** Delayed processing of new/changed files after changing the watch folder; possible event backlog.

**Suggestion:** Run the initial scan in a **dedicated thread or thread pool** (e.g. `std::thread::spawn`), and only start the filesystem watcher after the scan completes (or start the watcher immediately and merge “initial scan” results with events). That keeps the watcher loop responsive.

---

### 1.6 DB: multiple separate queries in `get_stats()`

**Where:** `get_stats()` in `src-tauri/src/db.rs`.

**Issue:** Six separate `query_row` calls (total + one per file type). Each takes the lock and runs a query.

**Impact:** Small but unnecessary lock contention and round-trips.

**Suggestion:** Use a single query with `GROUP BY file_type` (and a conditional for total count) or a small set of queries in one lock hold to reduce lock round-trips.

---

### 1.7 CPU temp / GPU usage: external processes every poll

**Where:** `get_cpu_temp_c()` and `get_gpu_usage_pct()` from `get_device_stats()`.

**Issue:** On macOS, `powermetrics` is run twice per device-stats sample (temp + GPU); on Linux, `nvidia-smi` and thermal file reads run every time. These add process spawn and I/O every 1.5 s when the overlay is on.

**Impact:** CPU and I/O; can be noticeable on battery.

**Suggestion:** Once device stats are moved to a background updater (see 1.1), consider **throttling** temp/GPU to a longer interval (e.g. every 5–10 s) and reusing the last value for intermediate overlay updates, since they change slowly.

---

## 2. Frontend (React / TypeScript)

### 2.1 Single large component and no memoization

**Where:** `App.tsx` (~3100 lines); no `useMemo`, `useCallback`, or `React.memo` in the app.

**Issue:** All state lives in `App`; any `setState` (e.g. device stats every 1.5 s, stream chunk updates, typing in search) causes the **entire** tree to re-render. Lists (chats, library files, messages, Kiwix catalog, map catalog, etc.) are recreated every time.

**Impact:** CPU (React reconciliation and DOM updates), especially with device overlay on (frequent stats updates) or during streaming (many chunk-driven updates).

**Suggestions:**

- **Device stats:** Keep overlay in a small component that only receives `deviceStats` and optional net speed; wrap it in `React.memo` so the rest of the app doesn’t re-render when only stats change.
- **Stable callbacks:** Use `useCallback` for handlers passed to children (e.g. `loadChat`, `handleAddPoi`, `onMapChatSend`) to avoid unnecessary child re-renders when parent re-renders for unrelated state.
- **Derived data:** Memoize filtered lists (e.g. `filteredChatList`, filtered Kiwix list) with `useMemo` so they aren’t recomputed every render.
- **List items:** For long lists (chat list, library files, messages), consider `React.memo` on list item components so only changed items re-render.

---

### 2.2 Chat list and library list: no virtualization

**Where:** Sidebar chat list (`filteredChatList.map`) and library file list (`libraryFiles.map`) in `App.tsx`.

**Issue:** Every item is rendered as a DOM node. With hundreds of chats or library files, this means hundreds of nodes and full re-renders when any parent state changes.

**Impact:** RAM and CPU when lists are large; scroll/layout can be heavier.

**Suggestion:** Use a **virtualized list** (e.g. `react-window`, `@tanstack/react-virtual`) for the chat list and for the library file list so only visible rows are mounted. This is highest impact when the user has many chats or many files.

---

### 2.3 Message list: no virtualization; key by index

**Where:** `messages.map((msg, i) => ( ... key={i} ... ))` in `App.tsx`.

**Issue:** (1) Every message is in the DOM; long conversations mean many nodes. (2) Using index as `key` can cause unnecessary re-renders or subtle bugs when messages are inserted or reordered.

**Impact:** CPU and RAM in long chats; possible extra reconciliation.

**Suggestions:** Use a **stable id** per message for `key` (e.g. generate an id when a message is added). For very long conversations, consider virtualizing the message list (e.g. only mount the last N messages plus a “load more” or scroll-to-top placeholder).

---

### 2.4 Streaming: frequent `setMessages` during chunks

**Where:** `listen("athena:stream-chunk", ...)` updates state with `setMessages((prev) => [...])` on every chunk.

**Issue:** Each chunk triggers a full re-render of `App` and the message list. During streaming there can be many chunks per second.

**Impact:** High CPU during streaming; UI can feel heavy.

**Suggestions:** (1) **Throttle or batch** chunk updates (e.g. coalesce chunks in a ref and call `setMessages` at 60 fps or every 50–100 ms). (2) Isolate the “current streaming message” in a small component or ref and only commit to `messages` when the stream ends (or on a timer), so the main message list re-renders less often.

---

### 2.5 Library panel: multiple invokes on open

**Where:** `useEffect(..., [libraryOpen])` runs `loadWatchFolder()`, `loadStats()`, `loadLibraryFiles()`, `get_map_catalog`, `get_downloaded_maps`.

**Issue:** Five separate invokes when the library panel opens. They are not awaited together (some are fire-and-forget), so there’s no single “batch” from the frontend’s perspective.

**Impact:** Several round-trips and backend calls; minor. Could be improved if backend exposed a single “library panel data” command that returns watch folder, index stats, library file list, map catalog, and downloaded maps in one call (reducing IPC and backend overhead).

**Suggestion:** Optional: add a backend command that returns all library-panel data in one response, and call it once when the panel opens. Alternatively, keep current API but run the existing invokes in parallel and ensure they’re not serialized unnecessarily (they already are largely parallel).

---

### 2.6 Sun times: geolocation + invoke every 10 minutes

**Where:** `useEffect` with `nightVisionMode` that calls `getPosition()` then `invoke("get_sun_times", ...)` and `setInterval(..., 10 * 60 * 1000)`.

**Issue:** When night vision is enabled (and not "off"), geolocation and sun-times computation run on a 10-minute interval. Acceptable, but worth noting.

**Impact:** Low; 10 min is reasonable. Optionally increase to 15–30 min if you want to reduce load a bit further.

---

### 2.7 Map: Leaflet and POI list

**Where:** `MapView.tsx`; `pois.map` for markers.

**Issue:** Map is only mounted when `mapOpen` is true. POI list is typically small. No major inefficiency; Leaflet is a known cost when the map is open.

**Impact:** Low for normal use. If the user had hundreds of POIs, consider clustering or limiting rendered markers.

---

## 3. Integration / Polling

### 3.1 Device overlay poll interval

**Where:** Frontend `setInterval(fetchStats, 1500)` when `deviceOverlayEnabled` is true.

**Issue:** 1.5 s is relatively aggressive when combined with the blocking backend work (see 1.1 and 1.7).

**Suggestion:** Increase to 2.5–3 s when overlay is on, especially after moving device stats to a background updater. Optionally make the interval configurable (e.g. in debug/developer settings).

---

### 3.2 Battery and sun times

**Where:** Battery every 3 min; sun times every 10 min when night vision is on.

**Issue:** Both are reasonable. No change required unless you want to extend intervals for battery savings.

---

## 4. Summary table (quick reference)

| Area | Issue | Impact | Effort |
|------|--------|--------|--------|
| Backend | Device stats block main thread + temp/GPU every poll | High | Medium (background updater + cache) |
| Backend | send_message_stream blocks before spawn | Medium | Low (move prep into thread) |
| Backend | get_system_specs refresh_all | Low | Low (narrow refresh) |
| Backend | Watcher initial_scan blocks | Medium on large folders | Medium (scan in separate thread) |
| Backend | get_stats() multiple queries | Low | Low (single query or grouped) |
| Frontend | No memoization; full app re-renders | High | Medium (memo + useCallback + useMemo) |
| Frontend | Streaming setMessages every chunk | High during stream | Medium (throttle/batch) |
| Frontend | No list virtualization | Medium for large lists | Medium (react-window or similar) |
| Frontend | Message list key by index | Low | Low (stable id) |
| Integration | Device overlay 1.5 s poll | Medium | Low (increase interval) |

---

## 5. Suggested order of work

1. **Device stats off main thread** (1.1) + **throttle temp/GPU** (1.7) + **slightly higher overlay interval** (3.1) – reduces main-thread blocking and process spawns.
2. **Throttle or batch stream chunk updates** (2.4) and **memoize device overlay** (2.1) – quick wins for CPU during streaming and with overlay on.
3. **send_message_stream: move prep into spawned thread** (1.4) – improves perceived responsiveness when sending.
4. **useCallback / useMemo for lists and handlers** (2.1) – reduces re-renders across the app.
5. **Virtualize chat list and/or library list** (2.2) if users report slowness with many items.
6. **Watcher initial scan in separate thread** (1.5) if you have users with very large watch folders.
7. **get_system_specs narrow refresh** (1.2) and **get_stats single query** (1.6) – small backend cleanups.

If you want to implement a subset, starting with (1) and (2) gives the largest benefit for device demand with moderate effort.
