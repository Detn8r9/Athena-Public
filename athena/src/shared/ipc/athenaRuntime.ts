/** True when running inside the Tauri webview (desktop host). */
export function isAthenaDesktopRuntime(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as Window & {
    __TAURI_INTERNALS__?: unknown;
    __TAURI__?: unknown;
  };
  return Boolean(w.__TAURI_INTERNALS__ || w.__TAURI__);
}

/** Browser / LAN client (not the native desktop shell). */
export function isAthenaWebRuntime(): boolean {
  return !isAthenaDesktopRuntime();
}

/**
 * Capabilities that require the desktop host (USB, native dialogs, serial).
 * Hidden or disabled when `isAthenaWebRuntime()`.
 */
export const HOST_ONLY_CAPABILITIES = [
  "usbExport",
  "usbFormat",
  "nativeFileDialog",
  "serialRadio",
  "meshUsb",
] as const;

export type HostOnlyCapability = (typeof HOST_ONLY_CAPABILITIES)[number];

export function isHostOnlyCapability(cap: HostOnlyCapability): boolean {
  void cap;
  return isAthenaWebRuntime();
}

export function canUseHostCapability(cap: HostOnlyCapability): boolean {
  void cap;
  return isAthenaDesktopRuntime();
}
