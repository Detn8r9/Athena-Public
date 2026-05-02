#!/usr/bin/env bash
# Fetch official kiwix-serve binaries from https://download.kiwix.org/release/kiwix-tools/
# and install them under src-tauri/bin/ with Tauri externalBin target triple names.
#
# Run from anywhere:
#   bash athena/src-tauri/scripts/fetch_kiwix_serve.sh
#
# Requires: curl, tar, unzip (for Windows archive), chmod (Unix).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_TAURI_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN="${SRC_TAURI_ROOT}/bin"
mkdir -p "${BIN}"

BASE="https://download.kiwix.org/release/kiwix-tools"
# Windows 3.8.2 win-x86_64 zip is not always published; 3.8.1 is available alongside Linux/macOS 3.8.2.
V_MAC_LINUX="3.8.2"
V_WIN="3.8.1"

tmpdir="$(mktemp -d)"
cleanup() { rm -rf "${tmpdir}"; }
trap cleanup EXIT

echo "==> macOS arm64 (aarch64-apple-darwin)"
if [ -f "${BIN}/kiwix-serve-aarch64-apple-darwin" ]; then
  echo "    (already present, skip)"
else
  curl -fsSL -o "${tmpdir}/mac.tgz" "${BASE}/kiwix-tools_macos-arm64-${V_MAC_LINUX}.tar.gz"
  mkdir -p "${tmpdir}/mac"
  tar -xzf "${tmpdir}/mac.tgz" -C "${tmpdir}/mac"
  mac_bin="$(find "${tmpdir}/mac" -type f -name kiwix-serve ! -path '*.app/*' | head -1)"
  test -n "${mac_bin}" && test -f "${mac_bin}"
  install -m0755 "${mac_bin}" "${BIN}/kiwix-serve-aarch64-apple-darwin"
fi

echo "==> Linux x86_64 (x86_64-unknown-linux-gnu)"
if [ -f "${BIN}/kiwix-serve-x86_64-unknown-linux-gnu" ]; then
  echo "    (already present, skip)"
else
  curl -fsSL -o "${tmpdir}/linux.tgz" "${BASE}/kiwix-tools_linux-x86_64-${V_MAC_LINUX}.tar.gz"
  mkdir -p "${tmpdir}/l"
  tar -xzf "${tmpdir}/linux.tgz" -C "${tmpdir}/l"
  linux_bin="$(find "${tmpdir}/l" -type f -name kiwix-serve | head -1)"
  test -n "${linux_bin}" && test -f "${linux_bin}"
  install -m0755 "${linux_bin}" "${BIN}/kiwix-serve-x86_64-unknown-linux-gnu"
fi

echo "==> Linux aarch64 (aarch64-unknown-linux-gnu)"
if [ -f "${BIN}/kiwix-serve-aarch64-unknown-linux-gnu" ]; then
  echo "    (already present, skip)"
else
  curl -fsSL -o "${tmpdir}/linux-arm.tgz" "${BASE}/kiwix-tools_linux-aarch64-${V_MAC_LINUX}.tar.gz"
  mkdir -p "${tmpdir}/la"
  tar -xzf "${tmpdir}/linux-arm.tgz" -C "${tmpdir}/la"
  linux_arm_bin="$(find "${tmpdir}/la" -type f -name kiwix-serve | head -1)"
  test -n "${linux_arm_bin}" && test -f "${linux_arm_bin}"
  install -m0755 "${linux_arm_bin}" "${BIN}/kiwix-serve-aarch64-unknown-linux-gnu"
fi

echo "==> Windows x86_64 (x86_64-pc-windows-msvc.exe)"
if [ -f "${BIN}/kiwix-serve-x86_64-pc-windows-msvc.exe" ]; then
  echo "    (already present, skip)"
else
  curl -fsSL -o "${tmpdir}/win.zip" "${BASE}/kiwix-tools_win-x86_64-${V_WIN}.zip"
  unzip -q -o "${tmpdir}/win.zip" -d "${tmpdir}/w"
  win_bin="$(find "${tmpdir}/w" -type f \( -name 'kiwix-serve.exe' -o -name 'kiwix-serve-windows.exe' \) | head -1)"
  if test -z "${win_bin}"; then
    win_bin="$(find "${tmpdir}/w" -type f -name '*.exe' | grep -i kiwix-serve | head -1)"
  fi
  test -n "${win_bin}" && test -f "${win_bin}"
  cp -f "${win_bin}" "${BIN}/kiwix-serve-x86_64-pc-windows-msvc.exe"
  chmod 0755 "${BIN}/kiwix-serve-x86_64-pc-windows-msvc.exe"
fi

echo "Done. Binaries:"
ls -la "${BIN}"/kiwix-serve-*
