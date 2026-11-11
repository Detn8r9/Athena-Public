# Athena MCP (local, read-mostly)

Stdio MCP server so Cursor can inspect Athena without clicking through the GUI. It does not talk to the Tauri window. It reads the repo, redacted local config, and loopback health endpoints (Ollama, Marketplace Whisper/Kokoro).

It will not delete data, print API keys, run a shell, or send chat.

## Enable in Cursor

Project config (already added at the repo root as `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "athena": {
      "command": "node",
      "args": [
        "/Users/you/Coding Projects/Project A.T.H.E.N.A/tools/athena-mcp/server.mjs"
      ]
    }
  }
}
```

Restart Cursor or reload MCP servers after saving. No `npm install` is required.

Optional env:

- `ATHENA_DATA_DIR` — AthenaData root if it is not `~/Library/Application Support/com.athena.micro`.

Check the probes without Cursor:

```bash
node tools/athena-mcp/server.mjs --self-test
```

## Tools

| Tool | What it does |
|------|----------------|
| `athena_status` | Repo version, data dir, DB/config presence, Ollama, vision names, voice ports |
| `athena_list_surfaces` | Static map of chat, conversation, vision, marketplace, map, library, mesh, settings, export |
| `athena_describe_chat` | How Quick/Deep, streaming, RAG, hold-to-talk, and vision are wired, plus known gaps |
| `athena_list_models` | `GET /api/tags` on loopback Ollama |
| `athena_voice_health` | Whisper `:9001/v1/models` and Kokoro `:8880/health`, or ports from marketplace state |
| `athena_vision_health` | Installed Ollama tags that match Athena's vision name hints |
| `athena_read_config` | `athena_config.json` with keys, tokens, birthdate, street address, and GPS fields replaced by `{ redacted, set }` |
| `athena_module_health` | HTTP GET of each installed Marketplace module health path on `127.0.0.1` |

`baseUrl` arguments must be `127.0.0.1` or `localhost`.
