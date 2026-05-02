export type MapMarkerCategory = "hazard" | "water" | "cache" | "med";

export const MAP_MARKER_CATEGORIES: Array<{ id: MapMarkerCategory; label: string }> = [
  { id: "hazard", label: "Hazard" },
  { id: "water", label: "Water" },
  { id: "cache", label: "Cache" },
  { id: "med", label: "Med" },
];

export function normalizeMapMarkerCategory(raw: string | undefined): MapMarkerCategory {
  const t = (raw ?? "hazard").trim().toLowerCase();
  if (t === "hazard" || t === "water" || t === "cache" || t === "med") {
    return t;
  }
  return "hazard";
}
