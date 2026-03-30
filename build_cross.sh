#!/usr/bin/env bash
# Build Docker cross image, run Tauri builds inside the container, and copy artifacts into
# athena/src-tauri/payload (mounted from the host, so outputs land in your repo).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
IMAGE="athena-cross-build"

echo "[build_cross] building image from Dockerfile.cross..."
docker build -f "$ROOT/Dockerfile.cross" -t "$IMAGE" "$ROOT"

echo "[build_cross] running builds (this may take a long time)..."
docker run --rm \
  -v "$ROOT:/workspace" \
  -w /workspace/athena \
  "$IMAGE" \
  bash -lc '
set -euo pipefail
export PATH="/usr/local/cargo/bin:$PATH"
export CI="true"

npm ci

PAYLOAD="/workspace/athena/src-tauri/payload"
mkdir -p "$PAYLOAD/Windows/64-bit" "$PAYLOAD/Windows/32-bit" "$PAYLOAD/Windows/ARM" \
         "$PAYLOAD/Linux/64-bit" "$PAYLOAD/Linux/ARM"

copy_if() {
  local src="$1"
  local dst="$2"
  if [[ -f "$src" ]]; then
    cp -f "$src" "$dst"
    echo "[build_cross] copied $(basename "$dst")"
  else
    echo "[build_cross] warning: missing $src (skipping)"
  fi
}

# Windows x86_64 GNU (binary only; skip installers in container)
if npm run tauri build -- --target x86_64-pc-windows-gnu --no-bundle; then
  copy_if "src-tauri/target/x86_64-pc-windows-gnu/release/athena.exe" \
    "$PAYLOAD/Windows/64-bit/ATHENA.exe"
else
  echo "[build_cross] warning: x86_64-pc-windows-gnu build failed"
fi

# Windows i686 GNU
if npm run tauri build -- --target i686-pc-windows-gnu --no-bundle; then
  copy_if "src-tauri/target/i686-pc-windows-gnu/release/athena.exe" \
    "$PAYLOAD/Windows/32-bit/ATHENA.exe"
else
  echo "[build_cross] warning: i686-pc-windows-gnu build failed"
fi

# Windows ARM64: no stable rustup mingw triple in this image; keep placeholder or manual build.
echo "[build_cross] note: Windows ARM binary not produced in this image; leave payload/Windows/ARM or build separately."

# Linux x86_64 AppImage (bundling enabled for this target)
if npm run tauri build -- --target x86_64-unknown-linux-gnu --ci; then
  shopt -s nullglob
  imgs=(src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/*.AppImage)
  if [[ ${#imgs[@]} -gt 0 ]]; then
    cp -f "${imgs[0]}" "$PAYLOAD/Linux/64-bit/ATHENA.AppImage"
    echo "[build_cross] copied Linux x86_64 AppImage"
  else
    echo "[build_cross] warning: no AppImage under bundle/appimage; check tauri bundle targets"
  fi
else
  echo "[build_cross] warning: x86_64-unknown-linux-gnu build failed"
fi

# Linux aarch64 (best-effort; may need extra sysroots)
if npm run tauri build -- --target aarch64-unknown-linux-gnu --ci; then
  shopt -s nullglob
  aimgs=(src-tauri/target/aarch64-unknown-linux-gnu/release/bundle/appimage/*.AppImage)
  if [[ ${#aimgs[@]} -gt 0 ]]; then
    cp -f "${aimgs[0]}" "$PAYLOAD/Linux/ARM/ATHENA.AppImage"
    echo "[build_cross] copied Linux aarch64 AppImage"
  fi
else
  echo "[build_cross] warning: aarch64-unknown-linux-gnu build failed (common without full cross libs)"
fi

echo "[build_cross] done. Payload tree is under $PAYLOAD on the host."
'

echo "[build_cross] finished."
