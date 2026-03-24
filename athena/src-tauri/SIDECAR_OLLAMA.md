# Ollama sidecar (macOS)

`tauri.conf.json` lists `ollama` in `bundle.externalBin`. Tauri expects these files next to `tauri.conf.json`:

- `ollama-aarch64-apple-darwin` (Apple Silicon)
- `ollama-x86_64-apple-darwin` (Intel Mac)

Replace the checked-in stubs with official Ollama CLI binaries renamed to match (same layout as the `pmtiles` sidecar pattern).

Capabilities: `capabilities/default.json` grants `shell:allow-spawn` and `shell:allow-kill` for the `ollama` sidecar (`sidecar: true`, args allowed for spawn).

Runtime: `src/lib.rs` spawns `ollama serve` during app setup (desktop), stores the `CommandChild` in managed `OllamaState`, drains command events on the async runtime, and calls `CommandChild::kill()` on `RunEvent::Exit`.
