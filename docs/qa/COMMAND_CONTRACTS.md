# Tauri command contracts (UI ↔ Rust)

Add or update a row when you add a `#[tauri::command]` that the frontend calls. Prefer stable **camelCase** argument names in TS `invoke` payloads (Tauri maps to Rust snake_case where configured).

| Command | Critical inputs | Success | Failure modes | UI surface |
|---------|-----------------|---------|---------------|------------|
| `get_resilience_status` | — | `ResilienceStatus` JSON | state missing → maintenance payload | `main.tsx` / maintenance gate |
| `get_error_log` | — | string (may be empty) | IO | Settings → Troubleshooting |
| `append_error_log` | `message` | unit | disk full | error logging |
| `export_diagnostics_json` | `destPath` | unit (writes UTF-8 JSON) | path empty, IO | Settings → Troubleshooting |
| `get_app_data_dir_path` | — | string path | portable resolution error | Troubleshooting → reveal folder |
| `backup_database` | `destPath` | unit | empty path, backup API error | Settings → General & Troubleshooting |
| `restore_database` | `sourcePath` | unit (reopens DB) | invalid SQLite, IO | Settings → General |
| `send_message_stream` | `message`, `history`, `chatId`, … | stream events | Ollama down, timeout | Chat |
| `search_rag_chunks` | `query`, `limit`, `filters` | chunk hits | DB locked, empty index | Chat / RAG |
| `get_pois` | — | list | DB | Map |
| `get_available_usbs` | — | list | permission / platform | Aegis / USB export |
| `export_athena_to_usb` | `destinationPath`, `exportType`, `wipeAfter`, `formatFirst`, `devicePath`, `expeditionBudgetBytes` | unit (writes portable tree) | disk full, sidecar missing (`pmtiles`), invalid tier | Aegis Protocol modal |
| `aegis_export_preflight` | `destinationPath` (optional mount path) | `AegisExportPreflight` JSON | DB backup for slim size, IO | Aegis Protocol modal (sizing + Expedition plan) |
| `get_map_display_settings` | — | `coordFormat`, `unitSystem`, `defaultTravelMode`, `lastKnownGpsLat`/`Lng` | IO | Settings → Maps; map gear popover (shared config) |
| `set_map_display_settings` | `value` (object above) | unit | IO | Settings → Maps |
| `map_assistant_send` | `user_message`, `context_json?` | `{ reply, toolResults }` | empty message, Ollama | Map → MapSmartBar |
| `list_saved_places` | — | list | session | Map → Saved places |
| `add_saved_place` | `name`, `lat`, `lng`, `icon`, `color`, `scope` | row | validation | Map → Saved places |
| `delete_saved_place` | `id` | unit | not owner | Saved places |
| `list_route_blockages` | `include_rejected` | list | session | Map → Blockages (optional) |
| `propose_route_blockage` | `geometry_json`, `reason`, `severity`, `modes_affected`, `expires_at?`, `notes` | row | rank &lt;2 | Blockages panel |
| `review_route_blockage` | `id`, `approved`, `review_notes?` | row | rank &lt;4 | Blockages panel |
| `valhalla_route` | `args`: `{ lat1, lon1, lat2, lon2, mode }` | route GeoJSON + `source` | no Valhalla → `straight_line_fallback` | Map → Route panel |
| `valhalla_get_status` | — | `reachable`, `message`, `activeRoutingRegionId?` | — | Route panel / Offline Map Manager |
| `valhalla_list_regions` | — | list | DB | Offline Map Manager → routing |
| `valhalla_set_active_region` | `region_id` | unit | IO | Routing section |
| `valhalla_register_routing_region` | `name`, `tiles_path`, `bbox_json`, `build_status` | row | auth | Routing section |
| `ai_list_staged` | `filters` (`status?`) | list | session | Settings → Maps (inbox) / `ApprovalInbox` |
| `ai_count_pending` | — | `u32` | rank &lt;4 → 0 | Map left rail badge |
| `ai_review_change` | `id`, `approved`, `review_notes?` | row | rank &lt;4 | Approval inbox |
| `ai_get_autoapprove_rules` / `ai_set_autoapprove_rule` / `ai_delete_autoapprove_rule` | per command | | rank 4+ / 5 for writes | Settings → Maps (Rank 5 rules) |
| `logistics_library_submit_message` | `message`, `attached?` (`mapLat`/`mapLon`/`savedPlaceId`/`locationText`), `forceApply?` | `entry`, `proposalLines`, `lineOutcomes` (per-line intake) | empty body, AI error, RBAC | Field notes (Logistics Library) |
| `logistics_library_list_entries` | — | list of logistics rows (author org + map fields) | session | Logistics Library sidebar |
| `apply_inventory_proposal` | `proposal_id` | unit (applies pending lines incl. `delta_quantity` / `add_item`) | rank / directorate | Logistics proposal UI / new-item modal |
| `reject_inventory_proposal` | `proposal_id` | unit | rank 3+ | Logistics |
| `update_inventory_directorate` | `item_id`, `new_directorate` | item row | rank 4+ | Quartermaster (re-bucket) |
| `inv_queue_count` | — | `{ pendingProposals, newItemPrompts, suppressedDuplicates }` | session | Inv. Queue chip (logistics header) |
| `add_personnel` / `admin_update_personnel` (mesh) | `directorate`, `tier`, `job_title` (optional) | | | Personnel hub org fields |

**`exportType` values:** `outpost` (aliases: `essentials`, `essentials_data`) — slim DB, no models, no example-region proximity extract; **USB app copy omits non-launcher directories under any bundled `payload/` tree** (still copies `Windows`/`Linux` launcher trees to the drive root) **and omits bundled `Resources/embedding` and `Resources/reranker` trees** (default weights from `bundle.resources` in `tauri.conf.json`). `bastion` (alias: `recommended`) — full app + full DB, models, example-region terrain extract, priority payload packs, maps + library excluding satellite rasters; `citadel` (aliases: `absolute`, `complete`) — full app + full payload to volume root and all maps/library; `expedition` (alias: `auto`) — budgeted greedy export; requires `expeditionBudgetBytes` (usable bytes after app copy, typically free space minus `expeditionSecurityBufferBytes` from preflight). `wipeAfter` (Scorched Earth) only when `exportType` is `citadel` / `absolute` / `complete`.

**Preflight:** `applicationBytes` is the full bundle; `applicationBundleBytesOutpost` is the smaller Outpost app copy size (see above).

**Diagnostics JSON** includes: app version/name, OS, arch, app data dir path, **truncated** error log excerpt, `openai_compatible_api_key_set` (boolean only), `ollama_running`, and full `resilience` snapshot. It must **not** contain API key material.
