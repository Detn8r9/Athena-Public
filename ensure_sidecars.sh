#!/bin/bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
TAURI_DIR="$ROOT/athena/src-tauri"
BIN_DIR="$TAURI_DIR/bin"
TAURI_CONF="$TAURI_DIR/tauri.conf.json"

mkdir -p "$BIN_DIR"

echo "Starting Omnibus Protocol Sidecar Verification..."

if [ ! -f "$TAURI_CONF" ]; then
  echo "[ERROR] Missing tauri config: $TAURI_CONF"
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "[ERROR] Node.js is required to parse tauri.conf.json externalBin."
  exit 1
fi

# macOS ships Bash 3.2 (no mapfile); build the array with read (Bash 4+ mapfile unavailable).
EXTERNAL_BINS=()
while IFS= read -r line || [ -n "$line" ]; do
  [ -z "$line" ] && continue
  EXTERNAL_BINS+=("$line")
done < <(node -e '
const fs = require("fs");
const p = process.argv[1];
const raw = fs.readFileSync(p, "utf8");
const j = JSON.parse(raw);
const arr = (j?.bundle?.externalBin ?? []);
for (const x of arr) if (typeof x === "string") console.log(x);
' "$TAURI_CONF")

echo "[ensure_sidecars] externalBin entries:"
for x in "${EXTERNAL_BINS[@]}"; do
  echo "  - $x"
done

need_bin() {
  local key="$1"
  for x in "${EXTERNAL_BINS[@]}"; do
    if [ "$x" = "$key" ]; then
      return 0
    fi
  done
  return 1
}

download_pmtiles() {
  cd "$BIN_DIR" || exit 1
  local ver="v1.19.0"
  if [ ! -f "pmtiles-x86_64-unknown-linux-gnu" ]; then
    echo "[DOWNLOADING] pmtiles Linux x86_64..."
    curl -L -s -o pmtiles-linux-x64.tar.gz "https://github.com/protomaps/go-pmtiles/releases/download/$ver/go-pmtiles_1.19.0_Linux_x86_64.tar.gz"
    tar -xzf pmtiles-linux-x64.tar.gz pmtiles && mv pmtiles pmtiles-x86_64-unknown-linux-gnu
    rm pmtiles-linux-x64.tar.gz
  else
    echo "[OK] pmtiles-x86_64-unknown-linux-gnu"
  fi

  if [ ! -f "pmtiles-aarch64-unknown-linux-gnu" ]; then
    echo "[DOWNLOADING] pmtiles Linux aarch64..."
    curl -L -s -o pmtiles-linux-arm.tar.gz "https://github.com/protomaps/go-pmtiles/releases/download/$ver/go-pmtiles_1.19.0_Linux_arm64.tar.gz"
    tar -xzf pmtiles-linux-arm.tar.gz pmtiles && mv pmtiles pmtiles-aarch64-unknown-linux-gnu
    rm pmtiles-linux-arm.tar.gz
  else
    echo "[OK] pmtiles-aarch64-unknown-linux-gnu"
  fi

  if [ ! -f "pmtiles-x86_64-pc-windows-msvc.exe" ]; then
    echo "[DOWNLOADING] pmtiles Windows x86_64..."
    curl -L -s -o pmtiles-win.zip "https://github.com/protomaps/go-pmtiles/releases/download/$ver/go-pmtiles_1.19.0_Windows_x86_64.zip"
    unzip -o -q -j pmtiles-win.zip pmtiles.exe && mv pmtiles.exe pmtiles-x86_64-pc-windows-msvc.exe
    rm pmtiles-win.zip
  else
    echo "[OK] pmtiles-x86_64-pc-windows-msvc.exe"
  fi
}

download_tesseract() {
  cd "$BIN_DIR" || exit 1
  local rel="tesseract-5.5.2"
  if [ ! -f "tesseract-x86_64-unknown-linux-gnu" ]; then
    echo "[DOWNLOADING] tesseract Linux x86_64..."
    curl -L -s -o tesseract-x86_64-unknown-linux-gnu "https://github.com/DanielMYT/tesseract-static/releases/download/$rel/tesseract.x86_64"
  else
    echo "[OK] tesseract-x86_64-unknown-linux-gnu"
  fi

  if [ ! -f "tesseract-aarch64-unknown-linux-gnu" ]; then
    echo "[DOWNLOADING] tesseract Linux aarch64..."
    curl -L -s -o tesseract-aarch64-unknown-linux-gnu "https://github.com/DanielMYT/tesseract-static/releases/download/$rel/tesseract.aarch64"
  else
    echo "[OK] tesseract-aarch64-unknown-linux-gnu"
  fi

  # macOS dev sidecar from local install
  local arch
  arch="$(uname -m)"
  local tauri_arch="$arch"
  if [ "$arch" = "arm64" ]; then
    tauri_arch="aarch64"
  fi
  local mac_sidecar="tesseract-$tauri_arch-apple-darwin"
  if [ ! -f "$mac_sidecar" ]; then
    if command -v tesseract >/dev/null 2>&1; then
      echo "[COPYING] tesseract macOS sidecar from local install..."
      cp "$(command -v tesseract)" "$BIN_DIR/$mac_sidecar"
    else
      echo "[WARNING] tesseract not installed on Mac. Run 'brew install tesseract' for local dev."
    fi
  else
    echo "[OK] $mac_sidecar"
  fi

  # Windows must be provided manually (licensing / packaging constraints)
  if [ ! -f "tesseract-x86_64-pc-windows-msvc.exe" ]; then
    echo ""
    echo "===================================================================="
    echo "[ACTION REQUIRED] Missing Windows Tesseract Binary!"
    echo "Provide this file manually in: $BIN_DIR"
    echo "Name must be EXACT: tesseract-x86_64-pc-windows-msvc.exe"
    echo "Suggested source (official installer): https://digi.bib.uni-mannheim.de/tesseract/"
    echo "===================================================================="
    exit 1
  else
    echo "[OK] tesseract-x86_64-pc-windows-msvc.exe"
  fi
}

ensure_pdftoppm() {
  cd "$BIN_DIR" || exit 1
  # pdftoppm is usually supplied by you via the documented Docker extraction steps.
  # Here we just enforce presence for the targets we build.
  local missing=0
  for f in \
    "pdftoppm-x86_64-unknown-linux-gnu" \
    "pdftoppm-aarch64-unknown-linux-gnu" \
    "pdftoppm-x86_64-pc-windows-msvc.exe"
  do
    if [ ! -f "$f" ]; then
      echo "[ACTION REQUIRED] Missing $f in $BIN_DIR"
      missing=1
    else
      echo "[OK] $f"
    fi
  done
  if [ "$missing" -ne 0 ]; then
    echo ""
    echo "[ensure_sidecars] pdftoppm is required by bundle.externalBin."
    echo "Follow the documented extraction steps (Ubuntu container) to populate these files."
    exit 1
  fi
}

ensure_ollama() {
  local ver="v0.20.3"
  if [ ! -f "$TAURI_DIR/ollama-x86_64-pc-windows-msvc.exe" ]; then
    echo "[DOWNLOADING] ollama Windows x86_64..."
    tmpdir="$(mktemp -d)"
    curl -L -s -o "$tmpdir/ollama-windows-amd64.zip" "https://github.com/ollama/ollama/releases/download/$ver/ollama-windows-amd64.zip"
    unzip -o -q -j "$tmpdir/ollama-windows-amd64.zip" 'ollama.exe' -d "$tmpdir"
    mv "$tmpdir/ollama.exe" "$TAURI_DIR/ollama-x86_64-pc-windows-msvc.exe"
    rm -rf "$tmpdir"
  else
    echo "[OK] ollama-x86_64-pc-windows-msvc.exe"
  fi
}

if need_bin "bin/pmtiles"; then
  download_pmtiles
fi

if need_bin "bin/tesseract"; then
  download_tesseract
fi

if need_bin "bin/pdftoppm"; then
  ensure_pdftoppm
fi

if need_bin "ollama"; then
  ensure_ollama
fi

chmod +x "$BIN_DIR"/* 2>/dev/null || true
chmod +x "$TAURI_DIR"/ollama-* 2>/dev/null || true

echo "Sidecar Verification Complete. All required externalBin entries are satisfied."

