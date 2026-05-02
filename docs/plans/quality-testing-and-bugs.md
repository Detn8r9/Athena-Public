# Plan: Quality, testing, and systematic bug prevention

**Scope:** Full stack—React UI, Tauri commands, SQLite, sidecars.  
**Related plans:** [resilience-ux-trust.md](resilience-ux-trust.md) (error boundaries, backup), [features-fast-wins.md](features-fast-wins.md) (diagnostics export for triage).

---

## Goals

- Repeatable **confidence** before shipping (manual + automated).
- Expand automated tests beyond the current **three** Vitest files:
  - [`athena/src/streamWatchdog.test.tsx`](../../athena/src/streamWatchdog.test.tsx)
  - [`athena/src/meshMessageLog.test.ts`](../../athena/src/meshMessageLog.test.ts)
  - [`athena/src/geographicBoundingBox.test.ts`](../../athena/src/geographicBoundingBox.test.ts)
- Document **command contracts** so UI and Rust stay aligned.

---

## Current state

| Area | Location | Notes |
|------|-----------|--------|
| Rust surface | [`athena/src-tauri/src/lib.rs`](../../athena/src-tauri/src/lib.rs) + modules | Large `invoke_handler`; domain logic split across files. |
| Client logging | `append_error_log`, `get_error_log` in lib.rs | Use for repro bundles and support. |
| CI | [`.github/workflows/`](../../.github/workflows/) | Verify `npm test` and `cargo test` run on PRs; extend if missing. |
| Integration tests | `athena/src-tauri/tests/` or repo `tests/` | Enumerate when implementing; add high-value cases. |

---

## Workstream A — Smoke test checklist (human, ~15 minutes)

**Purpose:** Catch “app won’t start” and broken primary flows without full regression.

**Environment:** Clean profile optional; note OS and build (debug vs release).

| Step | Action | Pass criteria |
|------|--------|----------------|
| 1 | Launch cold | Window appears; no immediate crash. |
| 2 | Maintenance gate | [`main.tsx`](../../athena/src/main.tsx) `get_resilience_status`: if maintenance, screen lists actionable items; else app loads. |
| 3 | Auth / lock | Unlock or sign-in path works; lock re-engages if applicable. |
| 4 | Chat | Send one message; no permanent spinner without error. |
| 5 | Map | Open map view; basemap or placeholder loads without blank screen. |
| 6 | Archive | Open Archive hub; list or empty state renders. |
| 7 | Settings | Change one non-destructive setting; persists after close/reopen. |
| 8 | USB list | Open Aegis/export flow; `get_available_usbs` returns or shows handled error (no uncaught exception). |

**Platforms (minimum):** macOS, Windows.

**Deliverable:** Commit checklist as `docs/qa/SMOKE_TEST.md` (or link from `CONTRIBUTING.md`).

---

## Workstream B — Unit tests (TypeScript)

**Priority modules (pure logic first)**

- `streamWatchdog`, `meshMessageLog`, geographic bbox helpers.
- Any **extracted hooks** from `App.tsx` after UI refactor (e.g. `useChatTransport`).

**Component tests**

- Use `@testing-library/react` + Vitest (already in devDependencies).
- **Invoke mocking:** Document pattern, e.g. `vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))` with typed responses per test.

**Target**

- **+10 meaningful tests** in first iteration (assertions on behavior, not snapshots-only).

---

## Workstream C — Rust tests

**Existing**

- Example: bbox tests in [`athena/src-tauri/src/portable.rs`](../../athena/src-tauri/src/portable.rs) (`#[cfg(test)]`).

**Extensions**

- Pure functions: path normalization, config merge, serialization round-trips.
- SQLite: use in-memory or tempdir for migration tests (avoid flaking on developer machines).

**Command**

- `cargo test` from `athena/src-tauri` in CI.

---

## Workstream D — Command contract table

Maintain a single markdown or CSV table (suggest `docs/qa/COMMAND_CONTRACTS.md`):

| Command | Critical inputs | Success shape | Failure modes | UI surface |
|---------|-----------------|---------------|---------------|------------|
| (example) `chat_console_stream` | messages, model id | stream events | Ollama down, timeout | Chat view |
| `search_rag_chunks` | query | hits | DB locked, empty index | Archive / chat |
| `get_downloaded_maps` | — | list | IO error | Map / offline manager |
| `export_athena_to_usb` | path, tier, flags | unit | disk full, permission | Aegis modal |
| `get_error_log` | — | string | — | Troubleshooting |

**Process:** Add a row when adding a new `#[tauri::command]`; review quarterly.

---

## Workstream E — Concurrency and resource scenarios

Document **expected** behavior (even if “not supported yet”):

1. **DB locked** during restore or concurrent write—UI message + no panic.
2. **Cancel** long download or export (if/when implemented)—resource cleanup.
3. **Parallel `invoke`** from UI (double-click)—idempotent or serialized on backend.

Run **manual** probes; file issues for gaps.

---

## Acceptance criteria

- [ ] Smoke checklist committed and linked from `docs/README.md` or `CONTRIBUTING.md`.
- [ ] CI runs **`npm test`** and **`cargo test`** on PR (or issue filed with owner if blocked).
- [ ] ≥ **10** new tests (TS and/or Rust) with clear names.
- [ ] Command contract table started with ≥ **5** commands.

---

## Suggested timeline

1. Smoke doc + CI verification (1 PR).
2. TS tests for pure modules (1–2 PRs).
3. Command contract table + Rust tests for chosen module (1 PR).
4. Expand table as features land.
