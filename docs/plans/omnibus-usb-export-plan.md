# Omnibus USB Export (Caesar-only encrypted backup)

## Purpose and relationship to existing USB export

ATHENA already ships `export_athena_to_usb` in `athena/src-tauri/src/portable.rs`, which builds a portable tree (application payload, embedding and reranker folders, optional full database and library mirrors) and is driven from Settings via `UsbExportModal` and `invoke("export_athena_to_usb", ...)`. The Omnibus feature is a separate Caesar-grade operation: it targets rapid backup and cloning of sensitive state (database, configuration, and local model weights) with mandatory encryption at rest on the destination. It should not replace the existing portable export unless you later choose to merge flows; the plan assumes a new Tauri command and UI section so behavior and permissions stay explicit.

## Task 1 Backend export command (`export.rs`)

Add a new Rust module `athena/src-tauri/src/export.rs` and register it from `lib.rs` with `mod export;` and a new `#[tauri::command]` in the handler list.

The command entrypoint should accept parameters the frontend can supply after user interaction: at minimum `destination_path` (string, absolute directory chosen in the UI), `export_password` (string, used only for key derivation and encryption; never persisted), and optionally a boolean such as `include_chat_models` if you want Caesar to exclude very large `models/chat` trees by default. The command must take `tauri::State<'_, auth::AuthSession>` (or the project’s equivalent session type) and call the same `authenticated_user` helper used elsewhere. If `user.rank_level < 5`, return `Err` immediately with a clear message (Caesar rank required). Rank 5 in this codebase is the COMMAND tier created as the default Caesar operator in `db.rs`.

Directory selection belongs on the frontend using the Tauri Dialog plugin, which is already a dependency (`tauri-plugin-dialog` in `Cargo.toml` and the web client plugin). The plan is: the React layer calls `open` from `@tauri-apps/plugin-dialog` with `directory: true` (and appropriate title), receives an absolute path string, then passes that string into the new command together with the password. The Rust command must validate that the path is non-empty, exists, and is a directory, and reject attempts to export into obvious unsafe locations if you add guardrails later (optional first version: document that Caesar is responsible for choosing removable media).

Because copying and encryption can take minutes, the command should not block the UI thread on the frontend: implement the heavy work in `tokio::task::spawn_blocking` or an async command pattern, and emit progress over Tauri events (for example `athena:omnibus-export-progress` with a small JSON payload: phase name, optional byte counts). Use `tauri::AppHandle` passed into helpers to call `app.emit`. Final success or failure should emit a terminal event or return `Result` from an async wrapper so the UI can reset state.

## Task 2 Data gathering and packaging

Resolve the ATHENA application data root the same way the rest of the app does: `portable::resolved_app_data_dir(&app)` (used today by `export_athena_to_usb`). From that root, the Omnibus packager should treat these as authoritative sources.

The SQLite index database is `athena_index.db` in the app data directory. Before reading, the implementation should coordinate with the shared `AppState` database mutex: either use SQLite’s online backup API (`rusqlite` already enables the `backup` feature) to a temporary file under the system temp directory, or briefly serialize writes through the existing `Database` lock and copy the file only when consistent. Document the chosen strategy in code comments; backup API is usually safest under concurrent readers.

System settings are stored as `athena_config.json` in the same app data directory (`CONFIG_FILENAME` in `config.rs`). Copy from disk after reading is enough for export; if you need a point-in-time snapshot while other code might write config, take the config lock used elsewhere (`AppState::config_lock`) before read-copy.

Local AI weights follow `ModelDirectoryPaths` in `config.rs`: under `models/chat` (GGUF and related chat model files), `models/embeddings/<effective_embedding_folder>` and `models/rerankers/<effective_reranker_folder>` where the effective folder names come from `effective_embedding_folder` and `effective_reranker_folder` on the loaded `AppConfig` via `load_config(&app_data_dir)`. The Omnibus bundle should mirror that structure under a single staging directory inside the chosen destination, for example `OmnibusExport_<ISO8601>/raw/` containing `athena_index.db` (or backup output), `athena_config.json`, and `models/...` with recursive directory copy. Reuse or factor shared recursive copy helpers from `portable.rs` where possible to avoid divergent behavior.

After staging, the encryption step (Task 3) writes final artifacts next to a short `README` or manifest JSON listing file names, cipher suite identifier, and KDF parameters so a future import tool can decrypt without guessing. Model weights may remain unencrypted only if you explicitly decide Omnibus is “DB and config only”; this plan assumes weights are included in the encrypted archive or encrypted as separate blobs for consistency with the user requirement.

## Task 3 Encryption strategy

Use the RustCrypto ecosystem already aligned with common Rust practice: add dependencies such as `aes-gcm` (AES-256-GCM) or `chacha20poly1305` (XChaCha20-Poly1305) for authenticated encryption. AES-256-GCM is widely understood for field operators and interoperates with many tools; XChaCha20-Poly1305 is an excellent alternative if you prefer avoiding AES-specific hardware assumptions. Pick one suite for v1 and record it in the manifest.

Raw passwords must not be used as the AES key. Derive a 256-bit key with a memory-hard password-based KDF: `argon2` (Argon2id) is the recommended choice, with randomly generated salt (16 or 32 bytes) stored in the manifest next to the ciphertext, plus explicit Argon2 parameters (iterations, memory KiB, lanes) versioned in the manifest. The export password is the value Caesar enters in the UI; it is independent of the login PIN unless you deliberately document “reuse PIN at your own risk.”

For each sensitive file (at minimum the database snapshot and `athena_config.json`), encrypt plaintext with a unique random nonce per file (12 bytes for GCM, appropriate length for XChaCha if chosen) prepended or stored in the manifest, and append the authentication tag as part of the ciphertext blob. Large trees (model folders) can be tarred or zipped to a single stream first using the existing `tar` / `zip` crates already in `Cargo.toml`, then encrypt the archive as one unit to simplify nonce management, or encrypt per file in a loop with per-file nonces listed in the manifest.

Zeroize password and key material after use (`zeroize` crate) to reduce exposure in memory. Do not log passwords or derived keys.

## Task 4 Frontend export interface

Extend Settings so a new subpanel (for example `settingsPanel === "omnibusExport"` alongside `usbExport`) appears only when `user.rank_level === 5` (mirror the backend check so lower ranks never see the nav item). Reuse `useAuth` rank from `App.tsx` patterns already used for Centurion-gated features.

The panel contains: a short warning that the export contains sensitive operational data; a password-type input for the encryption passphrase with confirmation field and minimum length hint; a read-only text field showing the chosen destination directory; a button that opens the directory picker via `@tauri-apps/plugin-dialog` `open` and stores the path in React state; and a primary “Start Omnibus export” button disabled until path and password validation pass.

While the command runs, show a progress region fed by `listen("athena:omnibus-export-progress", ...)` mapping phases to human-readable status lines, and disable destructive navigation if needed. On completion, show success or surface the error string from `invoke`. Clear password fields after completion.

Add lexicon strings in `athena/src/shared/lexicon.tsx` for both survival and fallout themes so labels stay consistent with the rest of Settings.

## Execution order when you implement

Implement `export.rs` helpers (path resolution, optional SQLite backup, staging copy, Argon2 plus AES-GCM or XChaCha, manifest write) and the gated command; wire events for progress; then add Cargo dependencies (`argon2`, `aes-gcm` or `chacha20poly1305`, `zeroize`, `rand` for salts if not already sufficient). Register the command in `lib.rs`. Finally add the Settings UI, dialog wiring, and event listeners. Add a short section to `athena/docs/MIGRATIONS.md` or internal security notes if you want operators to understand restore requirements.

This document is the review-ready plan; no application code changes are specified beyond file and module names above.
