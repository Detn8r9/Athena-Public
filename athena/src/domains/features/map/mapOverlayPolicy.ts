import { defaultTacticalLayerVisibilityState, type TacticalLayerVisibilityState } from "./TacticalSidebar";
import { classifyTacticalPoint } from "./tacticalMapClassify";
import { normalizeMapMarkerCategory } from "./mapMarkerTypes";

export function isTacticalDangerNotes(notes: string): boolean {
  return /\b(danger|hazard|blocked|obstruction|unsafe)\b/i.test(notes ?? "");
}

function catAllowsTactical(cat: TacticalLayerVisibilityState["overlayCategory"]): boolean {
  return cat === "all" || cat === "tactical";
}

function catAllowsResources(cat: TacticalLayerVisibilityState["overlayCategory"]): boolean {
  return cat === "all" || cat === "resources";
}

function catAllowsObstructions(cat: TacticalLayerVisibilityState["overlayCategory"]): boolean {
  return cat === "all" || cat === "obstructions";
}

export function markerCategoryAllowed(
  v: TacticalLayerVisibilityState,
  rawCategory: string | undefined,
): boolean {
  if (!catAllowsResources(v.overlayCategory)) return false;
  const c = normalizeMapMarkerCategory(rawCategory);
  const mk = v.mapMarkers ?? defaultTacticalLayerVisibilityState.mapMarkers;
  return mk[c];
}

export function tacticalPointAllowed(v: TacticalLayerVisibilityState, notes: string): boolean {
  const c = classifyTacticalPoint(notes);
  if (c.bucket === "network_pin") {
    return catAllowsTactical(v.overlayCategory) && v.network.nodes;
  }
  if (c.bucket === "resource") {
    return catAllowsResources(v.overlayCategory) && v.resources[c.kind];
  }
  // custom_pin
  if (isTacticalDangerNotes(notes)) {
    return catAllowsObstructions(v.overlayCategory) && v.dangerPins;
  }
  return catAllowsResources(v.overlayCategory) && v.customPins;
}

export function tacticalCoverageAllowed(v: TacticalLayerVisibilityState, notes: string): boolean {
  if (!catAllowsTactical(v.overlayCategory)) return false;
  if (!v.network.nodes || !v.network.coverageCircles) return false;
  return classifyTacticalPoint(notes).bucket === "network_pin";
}

export function meshAllowed(v: TacticalLayerVisibilityState): boolean {
  return catAllowsTactical(v.overlayCategory) && v.meshNodes && v.network.nodes;
}

export function peersAllowed(v: TacticalLayerVisibilityState): boolean {
  return catAllowsTactical(v.overlayCategory) && v.peerPositions;
}

export function drawingsAllowed(v: TacticalLayerVisibilityState): boolean {
  return catAllowsObstructions(v.overlayCategory) && v.drawings;
}

export function blockagesAllowed(v: TacticalLayerVisibilityState): boolean {
  return catAllowsObstructions(v.overlayCategory) && v.routeBlockages;
}

export function libraryPoisAllowed(v: TacticalLayerVisibilityState, layerFiltersPois: boolean): boolean {
  if (!layerFiltersPois) return false;
  return catAllowsResources(v.overlayCategory);
}
