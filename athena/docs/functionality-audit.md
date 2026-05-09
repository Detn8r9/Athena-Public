## Functional audit (automated + exploratory)

### What I can cover thoroughly

- **Automated correctness checks**: TypeScript build, Rust `cargo check`, Rust unit tests.
- **High-coverage UI smoke** (manual/exploratory): primary views, navigation, settings, queue/downloads, watch-folder, ingestion status UI.
- **Stress-style paths**: repeated downloads, cancellations, switching tabs mid-download, large ZIM ingestion, archive extraction, offline mode.

### Findings log (issues + notes)

#### 2026-05-09

- **Rust unit tests initially failing**: `db::tests::semantic_search_knn_includes_global_best_match` expected 50 results but retrieval now filters by similarity threshold. Adjusted test vectors to remain above the min threshold while preserving a unique global best match. **Status: fixed; tests now pass.**
- **Browser smoke (localhost/Vite, not Tauri webview) fails at login**: `TypeError: Cannot read properties of undefined (reading 'invoke')` and repeated `transformCallback` errors. This appears to be an environment mismatch (Tauri APIs not available in a normal browser). **Status: expected limitation; use the Tauri window for true end-to-end UI testing.**

### Automated results

- **`npm run build`**: PASS
- **`cargo check`**: PASS
- **`cargo test`**: PASS

