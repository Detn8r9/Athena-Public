# ATHENA smoke test (~15 minutes)

Human checklist before a release or risky merge. Record **OS**, **debug vs release**, and **git SHA** in your notes.

| Step | Action | Pass |
|------|--------|------|
| 1 | Cold launch | Window appears; no immediate crash. |
| 2 | Maintenance gate | If maintenance mode: failures are listed and actionable; else main shell loads. |
| 3 | Auth / lock | Unlock works; lock re-engages as expected. |
| 4 | Chat | Send one message; stream completes or shows a clear error (no endless spinner). |
| 5 | Map | Open Map (if enabled); **MapLibre** basemap loads (or online OSM); left rail (World / My location / Layers / Route / Block / Saved) responds; optional **Inbox** (rank 4+) opens; map chat sends. |
| 6 | Archive | Open Archive hub; tabs render; empty states OK. |
| 6b | Field & logistics | Open Field log & logistics: **Notes** composer accepts text; **Inventory** tab shows directorate-grouped stock (or empty); rank **4+** can open **Inv. Queue**; **View on map** on a geotagged entry emits focus (map recenters when Map is open). |
| 7 | Settings | Change one safe setting; close app and reopen; value persists. |
| 8 | Shortcuts | Press **?** or **Ctrl+/** (**⌘+/** on Mac) outside a text field; shortcuts modal opens and closes with Esc. |
| 9 | Troubleshooting | Settings → Troubleshooting: **Export diagnostics** saves JSON; **Open app data folder** opens the folder. |
| 10 | USB / Aegis | Open export flow; `get_available_usbs` succeeds or error is surfaced in UI (no uncaught exception). |

**Platforms (minimum):** macOS and Windows.

**Database backup drill (optional):** Settings → General → **Backup database**, then restore on a copy or test profile per [`COMMAND_CONTRACTS.md`](COMMAND_CONTRACTS.md) notes for `backup_database` / `restore_database`.
