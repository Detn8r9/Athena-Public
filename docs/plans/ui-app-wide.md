# Plan: App-wide UI, information architecture, and perceived performance

**Scope:** Entire ATHENA client (React + Tauri shell), not a single feature.  
**Related plans:** [bundle-size-and-slim-builds.md](bundle-size-and-slim-builds.md) (lazy loading overlaps), [features-fast-wins.md](features-fast-wins.md) (shortcuts / Help).

---

## Goals

- Reduce **cognitive load** across many hubs without changing core product scope.
- Improve **perceived performance** (first paint, time-to-interactive on common paths).
- Raise **accessibility** baseline (modals, live regions, keyboard paths).
- Make the codebase **easier to change** by shrinking the monolithic shell.

---

## Current state (grounded in repo)

| Area | Location | Notes |
|------|-----------|--------|
| View enum & labels | [`athena/src/appView.ts`](../../athena/src/appView.ts) | `AppView` union; `APP_VIEWS`, `APP_VIEW_LABELS`, startup persistence. |
| Main shell | [`athena/src/App.tsx`](../../athena/src/App.tsx) | `currentView` / `setCurrentView`; large conditional trees per view. |
| Entry | [`athena/src/main.tsx`](../../athena/src/main.tsx) | Eager `import App from "./App"`. |
| Bundler | [`athena/vite.config.ts`](../../athena/vite.config.ts) | No `manualChunks` / route splitting. |
| Heavy deps | [`athena/package.json`](../../athena/package.json) | `maplibre-gl`, `leaflet`, `pdf-lib`, `framer-motion`, `pmtiles`, etc. |
| Error containment | [`athena/src/ErrorBoundary.tsx`](../../athena/src/ErrorBoundary.tsx) | Supports `sectionLabel`; underused at view roots. |

---

## Workstream A — Navigation and wayfinding

**Objectives**

- Users always know **where they are** and how to return to a stable home.
- Non-fatal errors surface without blocking primary navigation.

**Actions**

1. **Header pattern:** Show current place using `APP_VIEW_LABELS[currentView]` (or equivalent) alongside existing `UniversalHeader` behavior; avoid duplicating titles in conflicting styles.
2. **Back to home:** One consistent control (keyboard + visible) from every hub to `home` (or last “safe” parent—document choice).
3. **Optional “last error” strip:** Subscribe to a lightweight store fed by `append_error_log` / local toast queue—dismissible, does not steal focus.
4. **Appendix (doc):** Add a short “Hubs vs tools” mental model to Help or internal docs: e.g. Archive = library + installers; Map = tactical view; Chat = AI console.

**Acceptance**

- New user can name their current section from the chrome alone.
- Strip does not break layout on small widths.

---

## Workstream B — Decompose `App.tsx` (incremental)

**Objectives**

- Stop unbounded growth of a single file; enable lazy loading and testing.

**Suggested structure (phased)**

```
athena/src/
  App.tsx                 # Thin: providers, shell, routing to lazy views
  AppShell.tsx            # Layout, nav, modals host
  views/
    HomeView.tsx
    ChatView.tsx
    MapView.lazy.tsx      # or re-export from features/map
    ...
```

**Practices**

- Extract **one view per PR** where possible; run full smoke after each.
- Prefer **domain contexts** (chat, map, mesh) over a single “god context.”
- **Milestone:** `App.tsx` **under ~2,000 lines** within 2–3 focused PRs (adjust number if already closer).

**Risks**

- **Circular imports:** Avoid `views/index.ts` re-exporting everything; export types only or use direct paths.
- **State lift:** Identify minimal props vs context per extraction.

---

## Workstream C — Lazy loading and chunk strategy

**Objectives**

- Smaller initial JS; faster first open on low-end hardware.

**Actions**

1. Wrap major views in `React.lazy(() => import(...))` + shared `<Suspense fallback={<ViewSkeleton />} />`.
2. In [`vite.config.ts`](../../athena/vite.config.ts), add `build.rollupOptions.output.manualChunks`, e.g. isolate:
   - `maplibre-gl`
   - `leaflet` + `react-leaflet` (if bundled together in practice)
   - `pdf-lib`
   - `pmtiles`
3. Capture **before/after** `vite build` output (chunk names and sizes); optionally add `rollup-plugin-visualizer` as dev-only.

**Acceptance**

- At least **two** heavy views load only when selected.
- No duplicate React or maplibre in multiple conflicting chunks (verify build).

---

## Workstream D — Map-specific UX

**Objectives**

- Pay map cost only when the map is needed.

**Actions**

1. Defer [`maplibrePmtilesProtocolRegistration`](../../athena/src/maplibrePmtilesProtocolRegistration.ts) (and similar) until map view mounts.
2. Evaluate **unmounting** `MapView` when `currentView !== "map"` vs keeping mounted (memory vs remount latency)—measure on target hardware.
3. Ensure map errors use `ErrorBoundary` with `sectionLabel="Tactical map"`.

---

## Workstream E — Accessibility

**Objectives**

- WCAG-oriented basics: focus, announcements, modals.

**Actions**

1. Audit modals: **Export**, **Mesh**, **POI**, **Settings** subpanels—focus trap, `Escape` to close, return focus to trigger.
2. Extend **`aria-live`** patterns used for export progress to:
   - Chat send failures (non-streaming)
   - Long-running map/archive downloads where applicable
3. Document primary shortcuts in Help (see features plan).

**Acceptance**

- Keyboard-only user can open Settings, change one option, close, and return to chat without trapped focus (spot check).

---

## Acceptance criteria (plan-level)

- [ ] First-load JS **reduced** vs baseline (document numbers in PR).
- [ ] All [`APP_VIEWS`](../../athena/src/appView.ts) reachable; startup view preference unchanged.
- [ ] ≥ **2** heavy views lazy-loaded.
- [ ] No production regression in lock/auth flow.

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Split PRs break types | Strict TS after each extraction; CI `tsc`. |
| Lazy boundaries flash empty | Shared skeleton component; tune fallback copy. |
| Map remount cost | Optional “keep warm” flag for power users later. |

---

## Suggested execution order

1. `manualChunks` + one lazy view (low risk).
2. Map deferral + ErrorBoundary on map root.
3. Incremental `App.tsx` extractions.
4. Accessibility audit pass on modals.
