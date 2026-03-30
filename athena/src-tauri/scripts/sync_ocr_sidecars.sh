#!/usr/bin/env bash
# Copies Homebrew pdftoppm + tesseract into src-tauri/bin (Tauri externalBin naming)
# and copies the full Homebrew tessdata tree into src-tauri/resources/tessdata.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STURI="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$STURI/bin"
RES_DIR="$STURI/resources"
mkdir -p "$BIN_DIR" "$RES_DIR"

TESSERACT_BIN="$(command -v tesseract)"
PDFTOPPM_BIN="$(command -v pdftoppm)"
if [[ -z "$TESSERACT_BIN" ]]; then
  echo "error: tesseract not found in PATH (install e.g. brew install tesseract)" >&2
  exit 1
fi
if [[ -z "$PDFTOPPM_BIN" ]]; then
  echo "error: pdftoppm not found in PATH (install e.g. brew install poppler)" >&2
  exit 1
fi

TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"
if [[ -z "$TARGET" ]]; then
  echo "error: could not determine rustc host triple" >&2
  exit 1
fi

if [[ "$(uname -s)" == MINGW* ]] || [[ "$(uname -s)" == *MSYS* ]] || [[ "$(uname -s)" == *CYGWIN* ]]; then
  DEST_TESS="$BIN_DIR/tesseract-${TARGET}.exe"
  DEST_PDF="$BIN_DIR/pdftoppm-${TARGET}.exe"
else
  DEST_TESS="$BIN_DIR/tesseract-${TARGET}"
  DEST_PDF="$BIN_DIR/pdftoppm-${TARGET}"
fi

cp -f "$TESSERACT_BIN" "$DEST_TESS"
cp -f "$PDFTOPPM_BIN" "$DEST_PDF"
chmod +x "$DEST_TESS" "$DEST_PDF" 2>/dev/null || true

echo "Synced OCR sidecars for target ${TARGET}:"
echo "  $DEST_TESS"
echo "  $DEST_PDF"

TESSDATA_SRC=""
if command -v brew >/dev/null 2>&1; then
  TESS_PREFIX="$(brew --prefix tesseract 2>/dev/null || true)"
  if [[ -n "$TESS_PREFIX" && -d "$TESS_PREFIX/share/tessdata" ]]; then
    TESSDATA_SRC="$TESS_PREFIX/share/tessdata"
  fi
fi
if [[ -z "$TESSDATA_SRC" ]]; then
  for p in /opt/homebrew/opt/tesseract/share/tessdata /usr/local/opt/tesseract/share/tessdata; do
    if [[ -d "$p" ]]; then
      TESSDATA_SRC="$p"
      break
    fi
  done
fi

TESSDATA_DEST="$RES_DIR/tessdata"
if [[ -n "$TESSDATA_SRC" ]]; then
  rm -rf "$TESSDATA_DEST"
  cp -R "$TESSDATA_SRC" "$TESSDATA_DEST"
  echo "Copied tessdata from: $TESSDATA_SRC -> $TESSDATA_DEST"
else
  echo "warning: could not locate Homebrew tessdata; create $TESSDATA_DEST manually" >&2
  mkdir -p "$TESSDATA_DEST"
fi
