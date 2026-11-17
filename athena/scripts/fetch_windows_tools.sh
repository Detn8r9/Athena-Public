#!/usr/bin/env bash
# Fetches the Windows helper programs together with the libraries they need, into
# src-tauri/bin/windows-tools/. The .exe files in src-tauri/bin/ alone do not start on Windows
# (kiwix-serve needs ICU, pdftoppm needs Poppler, tesseract needs Leptonica and friends).
#
# Install on the server (see docs/WINDOWS_SERVER_SETUP.md):
#   windows-tools/kiwix/*.dll      -> %LOCALAPPDATA%\ATHENA\            (next to kiwix-serve.exe)
#   windows-tools/tesseract/       -> %LOCALAPPDATA%\ATHENA\tools\tesseract\
#   windows-tools/pdftoppm/        -> %LOCALAPPDATA%\ATHENA\tools\pdftoppm\
#   windows-tools/ollama/lib/      -> %LOCALAPPDATA%\ATHENA\lib\     (lib\ollama\ggml-*.dll beside ollama.exe)
# Tesseract and Poppler keep their own folders so their DLLs never mix with ATHENA's.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="src-tauri/bin/windows-tools"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fetch() { # url sha256 file
  curl -fsSL -o "$WORK/$3" "$1"
  echo "$2  $WORK/$3" | shasum -a 256 -c - >/dev/null || { echo "Checksum mismatch: $1" >&2; exit 1; }
}

fetch "https://download.kiwix.org/release/kiwix-tools/kiwix-tools_win-x86_64-3.8.1.zip" \
  fcd01ed2b93e9a68632c7863c83b9f66bf64406a66357be1df7b8b75596f3e45 kiwix.zip
fetch "https://github.com/oschwartz10612/poppler-windows/releases/download/v26.09.0-0/Release-26.09.0-0.zip" \
  7a6f256a0ddf7536182246a5733331bf4677cbcc34f4663774947ad34556c8d0 poppler.zip
fetch "https://digi.bib.uni-mannheim.de/tesseract/tesseract-ocr-w64-setup-v5.3.0.20221214.exe" \
  175a326853f87474132c284072a821dd819c63b707b01879b309f80bd6c0ab1e tesseract-setup.exe
# Ollama's CPU backends (AVX2 etc.). Must match the bundled ollama.exe version (0.20.3).
# The zip is 2 GB because of the GPU libraries; only the small CPU ones are kept.
fetch "https://github.com/ollama/ollama/releases/download/v0.20.3/ollama-windows-amd64.zip" \
  d04069b24287aa33fb9abde930727d79e3101ba399b073206642ceb832eb1a5d ollama.zip

rm -rf "$OUT" && mkdir -p "$OUT/kiwix" "$OUT/pdftoppm" "$OUT/tesseract" "$OUT/ollama"
unzip -q "$WORK/kiwix.zip" -d "$WORK/kiwix"
cp "$WORK"/kiwix/icu*.dll "$OUT/kiwix/"
unzip -q "$WORK/poppler.zip" -d "$WORK/poppler"
cp "$WORK"/poppler/poppler-*/Library/bin/pdftoppm.exe "$WORK"/poppler/poppler-*/Library/bin/*.dll "$OUT/pdftoppm/"
# The Tesseract installer is an NSIS archive; 7-Zip unpacks it without running it.
7z x -y -o"$WORK/tesseract" "$WORK/tesseract-setup.exe" >/dev/null
cp "$WORK"/tesseract/tesseract.exe "$WORK"/tesseract/*.dll "$OUT/tesseract/"
unzip -q "$WORK/ollama.zip" 'lib/ollama/ggml-*.dll' -d "$OUT/ollama"
echo "Windows tools ready in $OUT"
