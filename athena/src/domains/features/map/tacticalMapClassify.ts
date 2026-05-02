/** Tactical POI / resource classification (no Leaflet). */

export type TacticalResourceKind = "power" | "water" | "food" | "equipment";

export function lucidePresetIconToResourceKind(lucideIconName: string): TacticalResourceKind | undefined {
  switch (lucideIconName) {
    case "Zap":
      return "power";
    case "Droplets":
      return "water";
    case "Apple":
      return "food";
    case "Home":
      return "equipment";
    default:
      return undefined;
  }
}

export function parseTacticalResourceKindLegacy(notes: string): TacticalResourceKind {
  const match = notes.trim().match(/^@(power|water|food|equipment)\b/i);
  if (match) {
    const token = match[1].toLowerCase();
    if (token === "power" || token === "water" || token === "food" || token === "equipment") {
      return token;
    }
  }
  return "equipment";
}

export function parseTacticalLucideIconName(notes: string): string | null {
  const m = notes.trim().match(/@icon:([A-Za-z0-9]+)/);
  return m ? m[1] : null;
}

export function parseTacticalNetworkCoverageRadiusMeters(notes: string): number | null {
  const m = notes.match(/@range_m:(\d+)/i);
  if (!m) return null;
  const n = Number.parseInt(m[1], 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

export function stripTacticalNotesBody(notes: string): string {
  return notes
    .replace(/@network\b/gi, "")
    .replace(/@icon:[A-Za-z0-9]+/gi, "")
    .replace(/@range_m:\d+\b/gi, "")
    .replace(/@(power|water|food|equipment)\b/gi, "")
    .replace(/\s+/g, " ")
    .trim();
}

type TacticalPointClassification =
  | { bucket: "network_pin" }
  | { bucket: "custom_pin" }
  | { bucket: "resource"; kind: TacticalResourceKind };

export function classifyTacticalPoint(notes: string): TacticalPointClassification {
  if (/@network\b/i.test(notes)) {
    return { bucket: "network_pin" };
  }
  const lucide = parseTacticalLucideIconName(notes);
  if (lucide) {
    const presetKind = lucidePresetIconToResourceKind(lucide);
    if (presetKind !== undefined) {
      return { bucket: "resource", kind: presetKind };
    }
    return { bucket: "custom_pin" };
  }
  return { bucket: "resource", kind: parseTacticalResourceKindLegacy(notes) };
}

export function parseTacticalResourceKind(notes: string): TacticalResourceKind {
  const c = classifyTacticalPoint(notes);
  if (c.bucket === "custom_pin" || c.bucket === "network_pin") {
    return "equipment";
  }
  return c.kind;
}

export function tacticalResourceColor(kind: TacticalResourceKind): string {
  switch (kind) {
    case "power":
      return "#fbbf24";
    case "water":
      return "#38bdf8";
    case "food":
      return "#4ade80";
    case "equipment":
    default:
      return "#c4b5fd";
  }
}
