# Project ATHENA — Polish & Technical Audit (Phase 0)

Generated as the baseline for themed polish, feature completion, and hardening. Re-run greps when closing phases.

## 1. Inline color literals (`rgba`, `hex`) in `athena/src/**/*.tsx`

| Area | Files (representative) | Notes |
|------|------------------------|-------|
| App shell / themes | `App.tsx` (PRESET_COLORS hex map — intentional for custom theme) | Theming pass: keep PRESET; replace ad-hoc panel borders with `var(--border)` |
| Personnel / profile | `PersonnelProfileModal.tsx` | `rgba(255,255,255,0.10)` on cards |
| Comms | `CommunicationsHub.tsx` | Distress input + list borders |
| Guest | `GuestDashboard.tsx` | Heavy inline hex/rgba — Phase 4 |
| Map | `MapViewMaplibre.tsx` (MapLibre paint; many hex) | MapLibre **requires** hex in layer paint; accept or map via theme CSS vars in TS |
| Map UI | `MapLeftRail`, `MapSmartBar`, `BlockagesPanel`, `SavedPlacesPanel`, `RoutePanel`, `ApprovalInbox` | Replace JSX inline with tokens / classes in Phase 2 |
| Other | `DashboardView`, `ExportModal`, `AcademyModule` (injected HTML CSS), `LaunchScreen`, `AegisExportIoPanel` | Phase 4 |

**Rerun:** `rg 'rgba\(|#[0-9a-fA-F]{3,8}' athena/src --glob '*.tsx'`

## 2. Rust `unwrap()` / `expect()` (non-exhaustive; high-risk call sites)

| File | Line(s) | Risk | Action |
|------|-----------|------|--------|
| `lib.rs` | 1677, 5744 | med / startup | Harden or document as invariant |
| `auth.rs` | 718 | high | `expect` on org chart — use `ok_or` |
| `db.rs` | 7623+ tests | test only | OK |
| `mesh_router.rs` | 555 | high | Handle `None` |
| `portable.rs` | 2952+ | migration path | Harden in Phase 5 |
| `mesh_listener.rs` | 1102 | low (post-clamp) | Replace with `?` or safe unwrap comment |
| `sdr_manager.rs` | 292+ | med | Use `map_err` for lock |
| `ai_tools.rs` | 139–142 | high | Validate coords before unwrap |

**Rerun:** `rg '\.unwrap\(\)|\.expect\(' athena/src-tauri/src --glob '*.rs'`

## 3. Tauri commands — clearance / rank checks (sample)

Commands that must enforce **clearance** or **rank** (verify per command when touching):

- `auth.rs`: `list_role_catalog`, `add_role_catalog_entry`, `get_personnel_profile`, personnel admin — rank/clearance mix; align with `require_clearance` helper where missing.
- `inventory.rs`, `blockages.rs`, `saved_places.rs`, export/ USB — confirm gates.
- `remote_queue::get_queue_status` — public read OK for operator console; no secrets.

**Action:** Add `require_clearance_level` in `auth.rs` and apply to L4+ notification APIs.

## 4. Map UI / behavior gaps

- **Overlays / tactical visibility**: `mergeTacticalLayerVisibility` — ensure reset-to-defaults persists consistently (audit `App.tsx` + `mapOverlayPolicy.ts`).
- **Blockages**: `BlockagesPanel` + `propose_route_blockage` / `review_route_blockage` — flow complete; confirm err UI uses theme tokens.
- **Broadcast listen**: no `start_broadcast_listen` in repo — Comms card is placeholder until SDR API exists; gate UX (Phase 1).

## 5. Frontend architecture hotspots

- **`App.tsx`**: Very large; mesh listeners, export state, and view switch should be extracted (Phase 5).
- **Duplicate `athena:mesh-text` listeners**: Audit `App.tsx` vs `CommunicationsHub` — dedupe (Phase 5).
- **Org chart**: Add pan/zoom (Phase 1); MapLibre colors optional theme bridge (Phase 2).

## 6. `setState` / perf (manual)

- Revisit heavy lists: org chart, personnel, inventory after Phase 1 features land.
- Prefer `useMemo` for derived commander/subordinate lists on client if any client-side tree remains.

---

**Next step:** Execute Phase 1 in code (profile enrichment, org chart pan/zoom, station AI DTO + listener, broadcast copy, map token pass).
