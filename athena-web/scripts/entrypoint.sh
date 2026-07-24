#!/usr/bin/env bash
set -euo pipefail

echo "[athena] starting core on ${ATHENA_HOST:-0.0.0.0}:${ATHENA_PORT:-8080}"

# Best-effort: ensure tiny local failover model exists in Ollama (non-blocking).
if [[ -n "${OLLAMA_BASE_URL:-}" ]]; then
  (
    sleep 3
    for i in $(seq 1 30); do
      if curl -fsS "${OLLAMA_BASE_URL}/api/tags" >/dev/null 2>&1; then
        MODEL="${ATHENA_LOCAL_MODEL:-qwen2.5:0.5b}"
        if ! curl -fsS "${OLLAMA_BASE_URL}/api/tags" | grep -q "${MODEL%%:*}"; then
          echo "[athena] pulling local stub model: ${MODEL}"
          curl -fsS "${OLLAMA_BASE_URL}/api/pull" -d "{\"name\":\"${MODEL}\"}" >/dev/null || true
        else
          echo "[athena] local model already present: ${MODEL}"
        fi
        break
      fi
      sleep 2
    done
  ) &
fi

exec python -m uvicorn backend.app.main:app \
  --host "${ATHENA_HOST:-0.0.0.0}" \
  --port "${ATHENA_PORT:-8080}" \
  --proxy-headers \
  --forwarded-allow-ips='*'
