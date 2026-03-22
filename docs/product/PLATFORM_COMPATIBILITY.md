# Platform compatibility (Windows, Linux, macOS)

This document summarizes how Athena behaves on **Windows**, **Linux**, and **macOS**. All three are supported; some features fall back gracefully when the OS doesn’t provide data.

---

## Core app

| Feature | Windows | Linux | macOS |
|--------|---------|--------|--------|
| Chat (Ollama) | ✅ | ✅ | ✅ |
| Library / watch folder / indexing | ✅ | ✅ | ✅ |
| Kiwix download & catalog | ✅ | ✅ | ✅ |
| Map, POIs, offline maps | ✅ | ✅ | ✅ |
| Themes, settings, config | ✅ | ✅ | ✅ |
| Chats (save/load/delete) | ✅ | ✅ | ✅ |
| File dialogs (Tauri) | ✅ | ✅ | ✅ |
| Local time / sun times | ✅ | ✅ | ✅ |

Paths (app data, config, DB, downloads) use the OS-appropriate locations via Tauri’s `app_data_dir()` (e.g. `%APPDATA%` on Windows, `~/.config` on Linux, `~/Library/Application Support` on macOS). The frontend uses `/[\\/]/` when splitting file paths so both `\` and `/` work.

---

## Device overlay (CPU, RAM, GPU, temp, network)

| Metric | Windows | Linux | macOS |
|--------|---------|--------|--------|
| CPU % | ✅ (sysinfo) | ✅ (sysinfo) | ✅ (sysinfo) |
| RAM (used/total) | ✅ (sysinfo) | ✅ (sysinfo) | ✅ (sysinfo) |
| CPU temperature | — (shows "—") | ✅ (thermal_zone / hwmon) | ⚠️ powermetrics (often needs root) |
| GPU usage | — (shows "—") | ✅ (nvidia-smi, NVIDIA only) | ⚠️ powermetrics (often needs root) |
| Network speed | ✅ (sysinfo; if 0 then 0) | ✅ | ✅ |
| Disk (SSD) | Currently 0 / not implemented | Same | Same |

- **Windows:** No CPU temp or GPU % in this build; overlay shows "—" for those. CPU and RAM work.
- **Linux:** CPU temp is read from `/sys/class/thermal/` or `/sys/class/hwmon/`. GPU % only for **NVIDIA** via `nvidia-smi`; AMD/Intel iGPU show "—".
- **macOS:** Temp and GPU % use `powermetrics`, which often requires running the app with elevated privileges; otherwise "—".

---

## Battery (low-battery banner, settings)

| Platform | Behavior |
|----------|----------|
| **macOS** | Uses `pmset -g batt`; percentage and charging state shown. |
| **Linux** | Uses `/sys/class/power_supply/` (e.g. BAT0/BAT1); percentage and charging state when available. |
| **Windows** | Not implemented; battery state is `None`, so the low-battery banner won’t show and settings show no percentage. |

---

## Ollama (detection & launch)

| Platform | Detection | Auto-launch attempt |
|----------|-----------|----------------------|
| **macOS** | HTTP to `127.0.0.1:11434` | Tries `/opt/homebrew/bin/ollama`, `/usr/local/bin/ollama`, `ollama`; then `open -a Ollama`. |
| **Windows** | Same | Tries `ollama` (PATH). |
| **Linux** | Same | Tries `/usr/bin/ollama`, `/usr/local/bin/ollama`, `ollama` (PATH). |

Ollama must be installed and (on Linux/macOS) often needs to be in PATH or in one of the listed paths.

---

## Build / runtime notes

- **Windows:** Release build uses `windows_subsystem = "windows"` so no console window appears. Icons: `icon.ico` is used for the app icon.
- **Linux:** No special subsystem. Ensure standard Rust/Tauri dependencies (e.g. GTK, WebKit) are installed per Tauri docs.
- **macOS:** Standard Tauri macOS app; `.icns` used for icon.

---

## Summary

- **Windows:** Full app works; device overlay shows CPU/RAM/network; battery, CPU temp, and GPU % show "—".
- **Linux:** Full app + battery + CPU temp + NVIDIA GPU % when available; AMD/Intel GPU show "—".
- **macOS:** Full app + battery; CPU temp and GPU % only when `powermetrics` is available (often with elevated privileges).

If you add Windows battery or GPU support later (e.g. WMI or a helper), this doc can be updated.
