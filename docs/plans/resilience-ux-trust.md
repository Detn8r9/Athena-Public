# Plan: Resilience, trust, and usability under failure

**Scope:** User trust when networks drop, processes crash, or disks misbehave.  
**Related plans:** [quality-testing-and-bugs.md](quality-testing-and-bugs.md) (smoke + concurrency), [features-fast-wins.md](features-fast-wins.md) (diagnostics), [ui-app-wide.md](ui-app-wide.md) (ErrorBoundary usage).

---

## Goals

- **Minimize data loss** (chat drafts, database).
- **Explain degradation** instead of silent failure.
- **Contain** UI faults so one view does not blank the entire station.
- **Document** operator-facing controls (maintenance, lock).

---

## Workstream A — Data safety

### A1 — Chat draft autosave

**Options**

- **localStorage:** debounced (300–500 ms) save of active chat input keyed by `chatId` or `draft:global`.
- **SQLite:** only if drafts must survive cache clears—heavier.

**Requirements**

- Restore draft when re-entering same chat; clear on successful send.
- Cap stored size (e.g. 32 KiB) to avoid quota issues.

**Files:** Chat composer in [`athena/src/App.tsx`](../../athena/src/App.tsx) or extracted `ChatView` when available.

### A2 — User-triggered database backup

**Backend**

- Command: copy `athena_index.db` to user-chosen path (dialog).
- Handle **WAL:** checkpoint or copy using SQLite backup API / `VACUUM INTO` / documented “close handles” pattern—align with existing portable / wipe code paths in [`athena/src-tauri/src/portable.rs`](../../athena/src-tauri/src/portable.rs) and DB module.

**Frontend**

- Troubleshooting: **“Backup database now…”**

**Docs**

- `docs/` or in-app: **restore** procedure (replace db while app closed; WAL files).

**Acceptance**

- One successful backup + restore drill logged in QA notes.

---

## Workstream B — Degradation messaging

**Concept:** `useBackendHealth()` or similar:

- Poll a **cheap** command (`get_system_stats`, ping, or dedicated `health_check`).
- Or derive from **recent `invoke` failures** (sliding window) to show a **non-blocking banner**: “Inference unavailable” / “Working offline.”

**CTA:** Link to Settings → AI, or retry button.

**Files:** [`athena/src/App.tsx`](../../athena/src/App.tsx) or new hook; Rust optional `health_check` returning `{ ollama: bool, ... }`.

**Acceptance**

- Banner clears when backend recovers without restart.

---

## Workstream C — Error boundary coverage

**Pattern:** [`ErrorBoundary`](../../athena/src/ErrorBoundary.tsx) with `sectionLabel`.

**Wrap first (high blast radius)**

1. Tactical / main map host ([`features/map/MapView.tsx`](../../athena/src/features/map/MapView.tsx) or parent in App).
2. Archive hub root ([`features/library-installer/ArchiveHub.tsx`](../../athena/src/features/library-installer/ArchiveHub.tsx)).
3. Mesh network hub root ([`MeshNetworkHubView.tsx`](../../athena/src/MeshNetworkHubView.tsx) or equivalent).

**Acceptance**

- Simulated throw in dev shows **section** name and Reload does not require full app restart (boundary reset only).

---

## Workstream D — Security UX

- **Lock screen:** Confirm no sensitive thumbnails or chat text in **window preview** / OS switcher where OS allows control.
- **Timeouts:** If session timeout exists, document; if not, consider optional auto-lock after N minutes (product decision).

---

## Workstream E — Maintenance mode

**Existing:** [`athena/src/main.tsx`](../../athena/src/main.tsx) calls `get_resilience_status`; [`MaintenanceScreen`](../../athena/src/MaintenanceScreen.tsx) when `maintenanceMode`.

**Docs**

- Operator procedure in [`docs/README.md`](../README.md): how to set/clear maintenance, where failures are logged.

---

## Plan-level acceptance criteria

- [ ] **Backup + restore** documented and **one** manual test recorded.
- [ ] ≥ **three** top-level view trees wrapped in `ErrorBoundary` with labels.
- [ ] Degradation **banner** or documented decision to defer (with issue link).

---

## Suggested order

1. ErrorBoundary on Map + Archive + Mesh.  
2. DB backup command + UI + doc.  
3. Chat drafts.  
4. Health banner.  
5. Security UX audit notes.
