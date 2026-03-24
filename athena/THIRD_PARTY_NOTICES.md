# Third-party notices

Athena includes or may interoperate with the following software. See each project's license for full terms.

## sqlite-vec

- **Project:** [sqlite-vec](https://github.com/asg017/sqlite-vec) (SQLite extension; Rust crate `sqlite-vec`).
- **Use:** Statically linked SQLite vector search extension for `chunk_embeddings_vec` virtual table (kNN RAG).
- **License:** See upstream repository (MIT-style license per project files).

## Kiwix tools (optional)

- **Project:** [kiwix-tools](https://github.com/kiwix/kiwix-tools) (`kiwix-search`, `kiwix-serve`, etc.).
- **Use:** Optional subprocess invocation when the user configures `kiwix_search_binary_path` in `athena_config.json`.
- **License:** GPL-3.0. If you redistribute binaries, comply with GPL (source offer, license text, etc.). Athena does not bundle these by default.

## GraphHopper / OSRM / Valhalla (future)

- Offline routing may use a sidecar engine (not bundled in-tree). License and attribution must be added if you ship a routing engine or graph data with the app.

## Moondream / vision (placeholder)

- Vision features are planned for local ONNX/Candle or sidecar; no model weights are bundled in this repository until explicitly added. Follow upstream model licenses when integrated.
