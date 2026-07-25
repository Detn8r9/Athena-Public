#!/usr/bin/env bash
# Smoke-test LAN bridge wave-1 commands (host must have Allow LAN access enabled).
# Usage: ./scripts/lan-bridge-smoke.sh [base_url]
set -euo pipefail
BASE="${1:-http://127.0.0.1:8787}"

invoke() {
  local cmd="$1"
  local args="${2:-{}}"
  local auth="${3:-}"
  local hdr=(-H "Content-Type: application/json")
  if [[ -n "$auth" ]]; then
    hdr+=(-H "Authorization: Bearer $auth")
  fi
  curl -sfS "${hdr[@]}" -d "{\"cmd\":\"$cmd\",\"args\":$args}" "$BASE/api/invoke"
}

echo "== health =="
curl -sfS "$BASE/api/health" | tee /tmp/athena-lan-health.json
echo

echo "== guest auth =="
AUTH_JSON=$(invoke authenticate_guest '{}')
echo "$AUTH_JSON" | tee /tmp/athena-lan-auth.json
TOKEN=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("token") or "")' <<<"$AUTH_JSON")
if [[ -z "$TOKEN" ]]; then
  echo "FAIL: no session token from authenticate_guest" >&2
  exit 1
fi
echo "token ok (${#TOKEN} chars)"

echo "== themes =="
invoke get_theme_config '{}' "$TOKEN" | tee /tmp/athena-lan-theme.json
echo

echo "== list_chats =="
invoke list_chats '{}' "$TOKEN" | tee /tmp/athena-lan-chats.json
echo

echo "== map read =="
invoke get_pois '{}' "$TOKEN" | tee /tmp/athena-lan-pois.json
echo
invoke get_map_markers '{}' "$TOKEN" | tee /tmp/athena-lan-markers.json
echo
invoke offline_basemap_paths_cmd '{}' "$TOKEN" | tee /tmp/athena-lan-basemap.json
echo

echo "== host-only blocked =="
BLOCK=$(curl -sS -H "Content-Type: application/json" -H "Authorization: Bearer $TOKEN" \
  -d '{"cmd":"export_athena_to_usb","args":{}}' "$BASE/api/invoke")
echo "$BLOCK"
python3 -c 'import json,sys; b=json.load(sys.stdin); assert b.get("ok") is False, b; print("usb export correctly blocked")' <<<"$BLOCK"

echo
echo "LAN smoke OK against $BASE"
