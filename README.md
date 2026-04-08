# ATHENA

Autonomous Tactical Heuristic Emergency Network Archive — a desktop application for offline-first knowledge, local AI assistance, maps, mesh radio workflows, and field operations tooling.

The runnable app lives in the `athena/` directory (Tauri 2, React, Rust).

## What it does (high level)

- Local chat with an on-device LLM (Ollama or a local OpenAI-compatible server), optional RAG over your own library folder
- Library indexing for PDFs, text, and Kiwix `.zim` archives; curated downloads via The Archive when configured
- Tactical map with offline basemaps, POIs, and optional Meshtastic overlays
- Meshtastic USB integration, mesh chat workspace, and configurable auto-reply
- Serial “Comms” workspace for radio terminals and logging
- Calendar (ICS import/export, templates, optional mesh-related flows)
- Academy lessons, personnel roster / PIN login, Logistics Hub (inventory / supply) where enabled
- Settings for themes, location context, backups, USB export, and diagnostics

## Resilience Layer (offline readiness)

On startup, ATHENA now performs a resilience check and will enter a non-destructive **Maintenance Mode** instead of crashing if required sidecars are missing or unhealthy. The backend also enables SQLite Write-Ahead Logging (WAL) and runs an `integrity_check` on each start to reduce the risk of silent corruption after power loss.

## Omnibus USB Export (tiered)

The Omnibus export supports three tiers:

- Essentials: intended to be small and fast, includes launchers plus a proximity-pruned PMTiles map extract around Fairlee, VT.
- Recommended: adds models and performs a priority-based payload fill until the destination drive would drop below a 500MB free-space reserve.
- Absolute: attempts to copy every file in the bundled payload (maximal transfer).

## Requirements

- [Rust](https://rustup.rs/) (stable)
- Node.js (LTS recommended)
- Platform prerequisites for [Tauri 2](https://v2.tauri.app/start/prerequisites/)

## Clone and run (development)

```bash
cd athena
npm install
npm run tauri dev
```

Production-style UI build (no native shell):

```bash
cd athena
npm install
npm run build
```

## Project layout


| Path                                 | Role                                                |
| ------------------------------------ | --------------------------------------------------- |
| `athena/package.json`                | Frontend scripts and dependencies                   |
| `athena/src/`                        | React / TypeScript UI                               |
| `athena/src-tauri/`                  | Rust backend, Tauri config, assets                  |
| `athena/branding/`                   | Icon sources for `npm run icon`                     |
| `docs/`                              | Additional documentation and notes                  |
| `athena/ATHENA_PROJECT_REFERENCE.md` | In-app reference used by the `/athena` chat command |


## Security and privacy

- Operator login uses a PIN verified against a local database; credentials are not sent to cloud services as part of that check.
- Inference and library search run on your machine unless you explicitly use networked features (e.g. map tile APIs, geocoding).
- Do not commit API keys, personal paths, or exported databases. Use `.env.example` patterns only for non-secret samples.

## Contributing

Use focused commits and keep secrets out of the repository. Run `npm run build` in `athena/` and `cargo check` in `athena/src-tauri` before opening a PR when you change code.

## License

Usage and redistribution terms are defined by the project owner; add a `LICENSE` file to the repository root when you publish one.

## Credits & acknowledgements

ATHENA is inspired by and built on top of work from the broader offline-first community. In particular:

- Project N.O.M.A.D.: survival computer inspiration and field-ready UX principles
- Protomaps: PMTiles format and mapping infrastructure
- Kolibri: offline education framework patterns (curriculum-first, device-local learning)