# Frontend (`src/`)

Vite + React 19. **Most UI is currently in `App.tsx`** (large single file); smaller focused components are split out below.

## Entry

| File | Role |
|------|------|
| **`main.tsx`** | React root mount |
| **`App.tsx`** | Main layout: chat, settings, library, map, mesh, themes, local data, etc. |
| **`App.css`** | Global + component styles for the app shell |

## Components & modules

| File | Role |
|------|------|
| **`MapView.tsx`** | In-app map (Leaflet) |
| **`OfflineMapManagerDashboard.tsx`** | Map manager modal: PMTiles/MBTiles catalog, download placeholders, viewport shell |
| **`offlineMapManagerCatalog.ts`** | `invoke` helper for `get_offline_map_manager_catalog` |
| **`HelpModal.tsx`** | Help categories / copy |
| **`MeshRadioModal.tsx`** | Mesh radio UI |
| **`PhotoshopColorPicker.tsx`** | Color picker widget |
| **`colorUtils.ts`** | Theme / color helpers |
| **`triageData.ts`** | Triage education content data |
| **`vite-env.d.ts`** | TypeScript env types |
| **`assets/`** | Bundled images (e.g. `react.svg`) |

## Public assets

Static files users or HTML reference live in **`../public/`** (e.g. `public/icons/`).

When adding a new top-level screen or feature, prefer **new files** here and **import into `App.tsx`** rather than growing unrelated code in one place.
