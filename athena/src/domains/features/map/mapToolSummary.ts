/** Text for map assistant tool results. */
export type MapToolResult = { tool: string; ok: boolean; error?: string; data?: unknown };

/** A marker the assistant suggests; saved only when the user confirms. */
export type MarkerProposal = { title: string; lat: number | null; lng: number | null; category: string; description: string };

export function markerProposalOf(t: MapToolResult): MarkerProposal | null {
  if (t.tool !== "add_marker" || !t.ok) return null;
  const p = (t.data as { proposal?: Partial<MarkerProposal> } | undefined)?.proposal;
  if (!p || typeof p.title !== "string" || !p.title.trim()) return null;
  return {
    title: p.title.trim(),
    lat: typeof p.lat === "number" ? p.lat : null,
    lng: typeof p.lng === "number" ? p.lng : null,
    category: typeof p.category === "string" && p.category ? p.category : "hazard",
    description: typeof p.description === "string" ? p.description : "",
  };
}

export function formatMapToolChipText(t: MapToolResult): string {
  if (!t.ok) return t.tool;
  if (t.tool === "add_marker") return "new marker";
  const d = t.data as
    | {
        ambiguous?: boolean;
        haversineMeters?: number;
        route?: {
          source?: string;
          distanceMeters?: number | null;
          durationSeconds?: number | null;
        } | null;
        distanceMeters?: number;
        durationSeconds?: number;
        source?: string;
      }
    | undefined;
  if (t.tool === "directions" && d) {
    if (d.ambiguous) return "directions · pick endpoint(s)";
    const route = d.route;
    const distM =
      (typeof route?.distanceMeters === "number" && Number.isFinite(route.distanceMeters)
        ? route.distanceMeters
        : null) ??
      (typeof d.haversineMeters === "number" && Number.isFinite(d.haversineMeters) ? d.haversineMeters : null);
    if (distM == null) return "directions";
    const mi = distM / 1609.34;
    const distStr = mi >= 10 ? `${mi.toFixed(0)} mi` : `${mi.toFixed(1)} mi`;
    const durS = route?.durationSeconds;
    if (typeof durS === "number" && Number.isFinite(durS) && (route?.source === "valhalla" || route?.source === "athena_router")) {
      return `directions · ${distStr} · ${Math.max(1, Math.round(durS / 60))}m`;
    }
    if (route?.source === "straight_line_fallback") {
      return `directions · ${distStr} · straight line`;
    }
    return `directions · ${distStr}`;
  }
  if (t.tool === "route" && d) {
    const distM =
      (typeof d.distanceMeters === "number" && Number.isFinite(d.distanceMeters) ? d.distanceMeters : null) ??
      null;
    const src = d.source;
    if (distM != null) {
      const mi = distM / 1609.34;
      const distStr = mi >= 10 ? `${mi.toFixed(0)} mi` : `${mi.toFixed(1)} mi`;
      const durS = d.durationSeconds;
      if (typeof durS === "number" && Number.isFinite(durS) && (src === "valhalla" || src === "athena_router")) {
        return `route · ${distStr} · ${Math.max(1, Math.round(durS / 60))}m`;
      }
      if (src === "straight_line_fallback") {
        return `route · ${distStr} · straight line`;
      }
      return `route · ${distStr}`;
    }
  }
  return t.tool;
}

/**
 * One line saying what the map assistant did when the model returned tool calls but no text.
 * Without it the bar cleared and nothing changed on screen, so a request looked ignored.
 */
export function summarizeMapToolResults(tools: MapToolResult[]): string {
  const lines = tools.map((t) => {
    if (!t.ok) return `Could not run ${t.tool.replace(/_/g, " ")}${t.error ? `: ${t.error}` : "."}`;
    const proposal = markerProposalOf(t);
    if (proposal) {
      return `Ready to add the marker "${proposal.title}" ${proposal.lat == null ? "at the centre of the map" : "at the position given"}. Choose Add marker to save it.`;
    }
    if ((t.tool === "search_markers" || t.tool === "search_pois") && Array.isArray(t.data)) {
      const rows = t.data as Array<{ title?: string; name?: string }>;
      const what = t.tool === "search_markers" ? "saved marker" : "place";
      if (rows.length === 0) return `No ${what}s matched.`;
      const names = rows
        .slice(0, 5)
        .map((r) => r.title ?? r.name)
        .filter(Boolean)
        .join(", ");
      return `Found ${rows.length} ${what}${rows.length === 1 ? "" : "s"}${names ? `: ${names}` : ""}.`;
    }
    return `Done: ${formatMapToolChipText(t)}.`;
  });
  return lines.join(" ");
}

