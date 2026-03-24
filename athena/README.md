# Athena (Tauri + React + TypeScript)

Run all npm/cargo commands from **this directory** (`athena/`).

## Where to look

| Path | Purpose |
|------|---------|
| **`src/`** | React UI — see [`src/README.md`](src/README.md) |
| **`src-tauri/src/`** | Rust backend — see [`src-tauri/src/README.md`](src-tauri/src/README.md) |
| **`src-tauri/tauri.conf.json`** | Tauri window, bundle, dev/build hooks |
| **`branding/`** | Square / squircle PNG sources for `npm run icon` |
| **`scripts/`** | Icon tooling (`squircle-mask.mjs`, `make-icon-source.mjs`) |
| **`public/`** | Static assets served by Vite |
| **`ATHENA_PROJECT_REFERENCE.md`** | Full app reference; **compiled into Rust** for the **`/athena`** chat prefix — keep in sync with real behavior |
| **`docs/THEMES.md`** | **Master theme guide** — palettes (`data-theme`), personas, Fallout CRT, survival manual vellum, night vision, Theme Studio FX, CSS variables |

## Repo-wide docs

Product notes, audits, and archives live one level up: **`../docs/`** (index: `../docs/README.md`).

## Commands

```bash
npm install
npm run dev          # Vite only
npm run tauri dev    # App with Rust backend
npm run tauri build  # Release bundle
npm run icon         # Regenerate squircle + platform icons from branding/athena-icon-square.png
```

## IDE

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
