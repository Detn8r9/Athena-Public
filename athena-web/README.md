# Athena Web

Fully local, modular AI command center — containerized dashboard for LAN access.

This is a **greenfield** web stack alongside the existing Tauri desktop app (`athena/`).
The base install stays lean (~core UI + orchestration); heavy AI modules download on demand.

## Stack

| Layer | Choice |
|--------|--------|
| Frontend | React + Vite + TypeScript + react-grid-layout |
| Backend | FastAPI (binds `0.0.0.0:8080`) |
| AI routing | LiteLLM cloud-first → strict Ollama failover |
| Voice | faster-whisper STT + Kokoro TTS (push-to-talk) |
| Media | ComfyUI (optional Compose profile) |
| Apps | Host Docker via `/var/run/docker.sock` + auto port mapping |
| Modules | Static YAML catalog + LAN HTTP peer pull |

## Quick start

```bash
cd athena-web
cp .env.example .env
# Add OPENAI_API_KEY / ANTHROPIC_API_KEY / etc. for cloud routing

docker compose up --build -d
# Open http://localhost:8080  (or http://<lan-ip>:8080)
```

### Optional ComfyUI profile

```bash
docker compose --profile comfyui up -d
```

### Local Ollama fallback

The `ollama` service is included by default. On first boot, the core pulls a tiny
`qwen2.5:0.5b` stub for offline failover (keeps the base image itself slim).

## Ports

| Service | Port |
|---------|------|
| Athena core (UI + API) | `8080` |
| Ollama | `11434` |
| ComfyUI (optional) | `8188` |

## Layout

```
athena-web/
  catalogs/          # modules.yaml, apps.yaml
  backend/           # FastAPI app
  frontend/          # React dashboard
  scripts/           # bootstrap / peer helpers
  docker-compose.yml
  Dockerfile
```

## Dashboard features (v1)

- Drag-and-drop widget grid (chat, modules, map stub, radio stub)
- Command bar (text launch)
- **More Apps** drawer for unpinned apps / Docker containers
- Module downloader (internet or LAN peer)
- Push-to-talk voice pipeline
