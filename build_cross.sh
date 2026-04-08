#!/usr/bin/env bash
# Build Docker cross image, run Tauri builds inside the container, and copy artifacts into
# athena/src-tauri/payload (mounted from the host, so outputs land in your repo).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
IMAGE_X64="athena-cross-build-x64"
IMAGE_ARM64="athena-cross-build-arm64"

echo "[build_cross] ensuring sidecar binaries..."
"$ROOT/ensure_sidecars.sh"

echo "[build_cross] building x64 image from Dockerfile.cross..."
# On Apple Silicon hosts, `docker build` defaults to linux/arm64.
# We must build a linux/amd64 image here so linuxdeploy-x86_64.AppImage can run.
docker buildx build --platform linux/amd64 -f "$ROOT/Dockerfile.cross" -t "$IMAGE_X64:latest" --load "$ROOT"

echo "[build_cross] building arm64 image from Dockerfile.cross.arm64..."
docker buildx build --platform linux/arm64 -f "$ROOT/Dockerfile.cross.arm64" -t "$IMAGE_ARM64:latest" --load "$ROOT"

echo "[build_cross] running builds (this may take a long time)..."
docker run --rm \
  --platform linux/amd64 \
  -v "$ROOT:/workspace" \
  -w /workspace/athena \
  -e PKG_CONFIG_ALLOW_CROSS=1 \
  "$IMAGE_X64:latest" \
  bash -lc '
set -euo pipefail
export PATH="/usr/local/cargo/bin:$PATH"
export CI="true"
export PKG_CONFIG_ALLOW_CROSS=1
 export APPIMAGE_EXTRACT_AND_RUN=1

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

# Windows x86_64 MSVC (cargo-xwin supplies Windows SDK libs for ONNX / MSVC toolchain)
if npm run tauri build -- --target x86_64-pc-windows-msvc --runner cargo-xwin --no-bundle; then
  copy_if "src-tauri/target/x86_64-pc-windows-msvc/release/athena.exe" \
    "$PAYLOAD/Windows/64-bit/ATHENA.exe"
else
  echo "[build_cross] warning: x86_64-pc-windows-msvc build failed"
fi

echo "[build_cross] note: Windows 32-bit GNU build removed (MSVC x64 only in this image); payload/Windows/32-bit unused."

# Windows ARM64: not produced in this image; leave payload/Windows/ARM or build separately.
echo "[build_cross] note: Windows ARM binary not produced in this image; leave payload/Windows/ARM or build separately."

# Linux x86_64 AppImage (bundling enabled for this target)
export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig
export APPIMAGE_EXTRACT_AND_RUN=1
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

echo "[build_cross] done. Payload tree is under $PAYLOAD on the host."
'

echo "[build_cross] running native arm64 Linux build..."
docker run --rm \
  --platform linux/arm64 \
  -v "$ROOT:/workspace" \
  -w /workspace/athena \
  "$IMAGE_ARM64:latest" \
  bash -lc '
set -euo pipefail
export PATH="/usr/local/cargo/bin:$PATH"
export CI="true"
export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
export APPIMAGE_EXTRACT_AND_RUN=1

npm ci

PAYLOAD="/workspace/athena/src-tauri/payload"
mkdir -p "$PAYLOAD/Linux/ARM"

if npm run tauri build -- --target aarch64-unknown-linux-gnu --ci; then
  shopt -s nullglob
  aimgs=(src-tauri/target/aarch64-unknown-linux-gnu/release/bundle/appimage/*.AppImage)
  if [[ ${#aimgs[@]} -gt 0 ]]; then
    cp -f "${aimgs[0]}" "$PAYLOAD/Linux/ARM/ATHENA.AppImage"
    echo "[build_cross] copied Linux aarch64 AppImage"
  else
    echo "[build_cross] warning: no ARM AppImage under bundle/appimage"
  fi
else
  echo "[build_cross] warning: aarch64-unknown-linux-gnu build failed"
fi
'

echo "[build_cross] finished."
