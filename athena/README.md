# Athena (Tauri + React + TypeScript)

## Project reference & `/athena` chat prefix

- **`ATHENA_PROJECT_REFERENCE.md`** — Full description of the app (architecture, features, settings, mesh, limits). It is **embedded at compile time** into the Rust backend (`include_str!`) for the **`/athena`** chat command.
- In chat, messages starting with **`/athena`** (case-insensitive) inject that reference into the system prompt and **skip normal library RAG** so the model answers from this document—ideal for questions about Athena itself.

---

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
