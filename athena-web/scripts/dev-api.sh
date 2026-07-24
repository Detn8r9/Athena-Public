#!/usr/bin/env bash
# Local dev without Docker: API on :8080, Vite on :5173 (proxies /api).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PYTHONPATH="$ROOT"
export ATHENA_CATALOG_DIR="$ROOT/catalogs"
export ATHENA_DATA_DIR="$ROOT/data"
export ATHENA_STATIC_DIR="$ROOT/frontend/dist"
export OLLAMA_BASE_URL="${OLLAMA_BASE_URL:-http://127.0.0.1:11434}"

cd "$ROOT"
if [[ ! -d .venv ]]; then
  python3 -m venv .venv
  . .venv/bin/activate
  pip install -r backend/requirements.txt
else
  . .venv/bin/activate
fi

exec python -m uvicorn backend.app.main:app --host 0.0.0.0 --port 8080 --reload
