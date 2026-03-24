# Phase audits (checklist)

Use after each milestone or release. See also `PHASE0_BASELINE.md`.

## Automated

- `cd athena/src-tauri && cargo test`
- `cd athena && npm run build`

## Manual smoke

- [ ] Fresh install / first launch
- [ ] Chat (Ollama + optional OpenAI-compatible local)
- [ ] RAG / library ingest + retrieval
- [ ] Maps / POIs
- [ ] Settings: user profile, inference backend, 18+ NSFW catalog (when applicable)
- [ ] Help assistant
- [ ] Mesh (if hardware)
- [ ] Upgrade: backup `athena_config.json` + `athena_index.db` before major upgrades

## Security

- OpenAI-compatible URL must stay localhost-only (see `openai_chat.rs`).
- Do not log API keys or full request bodies with secrets.
- Subprocess/binary paths: `kiwix-search` uses validated `Path` (no shell).

---

## Cross-cutting

- **Config/schema:** `config_schema_version` in `AppConfig`; age gate clears `allow_nsfw_unrestricted_models` when under 18.
- **Docs:** Help modal + this folder for user-facing limitations.
- **Legal:** `THIRD_PARTY_NOTICES.md` for bundled or documented tools (sqlite-vec, Kiwix GPL, etc.).
