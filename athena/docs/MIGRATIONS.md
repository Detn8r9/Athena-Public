# Config and database migrations

- **Config:** `athena_config.json` in the app data directory. New fields use `#[serde(default)]` for backward compatibility. `allow_nsfw_unrestricted_models` is forced off when the user is not 18+ (from birthdate).
- **Database:** `athena_index.db` — schema evolves with `CREATE TABLE IF NOT EXISTS` and occasional `ALTER` attempts in `migrate_map_schema` patterns. The `chunk_embeddings_vec` sqlite-vec table is created when possible; `rebuild_chunk_embeddings_vec` can resync from `chunk_embeddings`.
- **Backup:** Before major upgrades, copy both `athena_config.json` and `athena_index.db` (see Phase audits checklist).
