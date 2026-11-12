/** Small display helpers for the Vault screen (kept apart so they can be unit-tested). */

export type CoverageLevel = "none" | "thin" | "some" | "good";
export type AlertLevel = "info" | "watch" | "warning" | "act";

export const LEVEL_LABEL: Record<CoverageLevel, string> = {
  none: "Nothing",
  thin: "Thin",
  some: "Some",
  good: "Good",
};

export const ALERT_LABEL: Record<AlertLevel, string> = {
  info: "Info",
  watch: "Watch",
  warning: "Warning",
  act: "Decide now",
};

export function levelTone(level: CoverageLevel): "bad" | "warn" | "good" | undefined {
  if (level === "none") return "bad";
  if (level === "thin") return "warn";
  if (level === "good") return "good";
  return undefined;
}

export function alertTone(level: AlertLevel): "bad" | "warn" | undefined {
  if (level === "act") return "bad";
  if (level === "warning") return "warn";
  return undefined;
}

/** 1536 -> "1.5 KB"; matches the backend's wording. */
export function formatBytes(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let v = n;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u += 1;
  }
  if (u === 0) return `${Math.round(n)} bytes`;
  return v >= 100 ? `${v.toFixed(0)} ${units[u]}` : `${v.toFixed(1)} ${units[u]}`;
}

/** Kiwix listings write sizes as "15M" or "1.2G"; show them as "15 MB". */
export function listingSize(text: string | null | undefined): string {
  const m = /^\s*([\d.]+)\s*([KMGT])B?\s*$/i.exec(text ?? "");
  if (!m) return text?.trim() || "—";
  return `${m[1]} ${m[2].toUpperCase()}B`;
}
