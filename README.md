# Athena

Desktop app (**Tauri 2** + **React** + **Rust**). Day-to-day development happens inside the **`athena/`** folder.

## Quick map

| Path | What it is |
|------|------------|
| **`athena/`** | Application root: `package.json`, `src/` (UI), `src-tauri/` (Rust). Run `npm install`, `npm run tauri dev` from here. |
| **`athena/src/`** | React + TypeScript UI (see `athena/src/README.md`). |
| **`athena/src-tauri/src/`** | Rust backend crates as modules (see `athena/src-tauri/src/README.md`). |
| **`athena/branding/`** | Source app icons (square/squircle PNGs) used by `npm run icon`. |
| **`docs/`** | Product notes, audits, and archived reference material (see `docs/README.md`). |
| **`.cursor/rules/`** | Cursor project rules (optional). |

## Docs

- **Living app description** (also embedded for the `/athena` chat command): `athena/ATHENA_PROJECT_REFERENCE.md`
- **Everything else**: start at [`docs/README.md`](docs/README.md)

## Clone & run

```bash
cd athena
npm install
npm run tauri dev
```

Requires [Rust](https://rustup.rs/), Node.js, and platform tools per [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
