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
- Settings for themes, location context, backups, **Aegis Protocol** (tiered USB export), and diagnostics

## Resilience Layer (offline readiness)

On startup, ATHENA performs a resilience check and can enter a non-destructive **Maintenance Mode** instead of crashing when required sidecars are missing or unhealthy (see backend diagnostics). SQLite uses **Write-Ahead Logging (WAL)** and an `integrity_check` on each start to reduce the risk of silent corruption after power loss.

## Aegis Protocol (tiered USB export)

Aegis Protocol supports three tiers:

- **Essentials**: small and fast — platform launchers from the bundled payload, a **25-mile PMTiles extract** centered on **Fairlee, VT** (43.908°N, 72.143°W), and a copy of the local **`athena_index.db`** when it exists (personnel / roster / library index). Does not copy embedding or reranker models or full document mirrors.
- **Recommended**: copies embedding and reranker models, then **priority-based** payload packs until free space would drop below a **500MB** reserve.
- **Absolute**: copies the full bundled payload tree and mirrors watched library folders plus the index database (see in-app export notes for wipe / “scorched earth” options).

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

- **Project N.O.M.A.D.**: survival computer inspiration and field-ready UX principles
- **Protomaps**: PMTiles format and mapping infrastructure (see also [source.coop Protomaps OSM](https://data.source.coop/protomaps/openstreetmap/tiles/v3.pmtiles) for planet extracts)
- **Kolibri** (Learning Equality): offline education *platform* patterns — curriculum-first, device-local learning (distinct from third-party curriculum PDFs)
- **Core Knowledge**: educational sequencing and knowledge-building curriculum *framework* (operator-supplied materials must comply with Core Knowledge licensing)
- **CK-12 Foundation**: open STEM curriculum resources where used — verify [CK-12 terms](https://www.ck12.org/) for any content you bundle or redistribute

Bundled Academy PDFs under `athena/src-tauri/payload/Academy/` are **not** shipped by default; add `reader.pdf` / `lecture_guide.pdf` per unit only under licenses you are allowed to use.