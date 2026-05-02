import React, { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openNativeFileDialog } from "@tauri-apps/plugin-dialog";
import {
  CircleDot,
  DatabaseBackup,
  Link2,
  MapPin,
  OctagonAlert,
  Pencil,
  RadioTower,
  Shield,
  Package,
  Sparkles,
  Upload,
  Utensils,
  Waves,
  Zap,
  Droplets,
} from "lucide-react";
import {
  defaultTacticalLayerVisibilityState,
  mergeTacticalLayerVisibility,
  type MapOverlayCategory,
  type TacticalLayerVisibilityState,
} from "./TacticalSidebar";
import { MAP_MARKER_CATEGORIES } from "./mapMarkerTypes";

type TacticalImportResult = {
  imported_points_of_interest: number;
  imported_mesh_node_locations: number;
  source_path: string;
};

function allLayersOff(): TacticalLayerVisibilityState {
  return mergeTacticalLayerVisibility({
    overlayCategory: "all",
    activeFamily: "network",
    network: { nodes: false, coverageCircles: false, links: false },
    resources: { power: false, water: false, food: false, equipment: false },
    customPins: false,
    meshNodes: false,
    peerPositions: false,
    mapMarkers: { hazard: false, water: false, cache: false, med: false },
    drawings: false,
    routeBlockages: false,
    dangerPins: false,
  });
}

function isFullyVisible(v: TacticalLayerVisibilityState): boolean {
  const d = defaultTacticalLayerVisibilityState;
  if (v.overlayCategory !== "all") return false;
  return (
    v.meshNodes === d.meshNodes &&
    v.peerPositions === d.peerPositions &&
    v.network.nodes === d.network.nodes &&
    v.network.coverageCircles === d.network.coverageCircles &&
    v.network.links === d.network.links &&
    v.resources.power === d.resources.power &&
    v.resources.water === d.resources.water &&
    v.resources.food === d.resources.food &&
    v.resources.equipment === d.resources.equipment &&
    v.customPins === d.customPins &&
    v.mapMarkers.hazard === d.mapMarkers.hazard &&
    v.mapMarkers.water === d.mapMarkers.water &&
    v.mapMarkers.cache === d.mapMarkers.cache &&
    v.mapMarkers.med === d.mapMarkers.med &&
    v.drawings === d.drawings &&
    v.routeBlockages === d.routeBlockages &&
    v.dangerPins === d.dangerPins
  );
}

type GridItem = {
  id: string;
  label: string;
  category: MapOverlayCategory;
  icon: React.ReactNode;
};

export function MapOverlaysPanel({
  panelOpen,
  onClose,
  tacticalLayerVisibility,
  onTacticalLayerVisibilityChange,
  layerFilters,
  setLayerFilters,
  onOpenBlockagesManager,
  onTacticalDataImported,
}: {
  panelOpen: boolean;
  onClose: () => void;
  tacticalLayerVisibility: TacticalLayerVisibilityState;
  onTacticalLayerVisibilityChange: (next: TacticalLayerVisibilityState) => void;
  layerFilters: { pois: boolean; roads: boolean; rivers: boolean };
  setLayerFilters: React.Dispatch<React.SetStateAction<{ pois: boolean; roads: boolean; rivers: boolean }>>;
  onOpenBlockagesManager: () => void;
  onTacticalDataImported?: () => void;
}) {
  const [statusMessage, setStatusMessage] = useState("");
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);

  /** Always merge so older persisted shapes (or partial updates) cannot omit `mapMarkers` and crash the panel. */
  const v = useMemo(() => mergeTacticalLayerVisibility(tacticalLayerVisibility), [tacticalLayerVisibility]);
  const allOn = useMemo(() => isFullyVisible(v), [v]);

  const gridItems: GridItem[] = useMemo(
    () => [
      { id: "mesh", label: "Mesh nodes", category: "tactical", icon: <RadioTower size={18} /> },
      { id: "peers", label: "Live peers", category: "tactical", icon: <Sparkles size={18} /> },
      { id: "nodes", label: "Network points", category: "tactical", icon: <MapPin size={18} /> },
      { id: "cov", label: "Coverage rings", category: "tactical", icon: <CircleDot size={18} /> },
      { id: "links", label: "Links (reserved)", category: "tactical", icon: <Link2 size={18} /> },
      { id: "power", label: "Power sites", category: "resources", icon: <Zap size={18} /> },
      { id: "water", label: "Water sites", category: "resources", icon: <Droplets size={18} /> },
      { id: "food", label: "Food sites", category: "resources", icon: <Utensils size={18} /> },
      { id: "equipment", label: "Equipment / bases", category: "resources", icon: <Package size={18} /> },
      { id: "customPins", label: "Custom pins", category: "resources", icon: <MapPin size={18} /> },
      { id: "mkHazard", label: "Markers: hazard", category: "resources", icon: <OctagonAlert size={18} /> },
      { id: "mkWater", label: "Markers: water", category: "resources", icon: <Waves size={18} /> },
      { id: "mkCache", label: "Markers: cache / outposts", category: "resources", icon: <Package size={18} /> },
      { id: "mkMed", label: "Markers: medical", category: "resources", icon: <Shield size={18} /> },
      { id: "libPois", label: "Library POIs", category: "resources", icon: <MapPin size={18} /> },
      { id: "drawings", label: "Drawings", category: "obstructions", icon: <Pencil size={18} /> },
      { id: "blockages", label: "Route blockages", category: "obstructions", icon: <OctagonAlert size={18} /> },
      { id: "dangerPins", label: "Hazard-tagged pins", category: "obstructions", icon: <OctagonAlert size={18} /> },
    ],
    [],
  );

  function itemEnabled(item: GridItem): boolean {
    if (v.overlayCategory === "all") return true;
    return v.overlayCategory === item.category;
  }

  function toggleGridItem(id: string) {
    const next = mergeTacticalLayerVisibility(v);
    switch (id) {
      case "mesh":
        next.meshNodes = !next.meshNodes;
        break;
      case "peers":
        next.peerPositions = !next.peerPositions;
        break;
      case "nodes":
        next.network = { ...next.network, nodes: !next.network.nodes };
        break;
      case "cov":
        next.network = { ...next.network, coverageCircles: !next.network.coverageCircles };
        break;
      case "links":
        next.network = { ...next.network, links: !next.network.links };
        break;
      case "power":
        next.resources = { ...next.resources, power: !next.resources.power };
        break;
      case "water":
        next.resources = { ...next.resources, water: !next.resources.water };
        break;
      case "food":
        next.resources = { ...next.resources, food: !next.resources.food };
        break;
      case "equipment":
        next.resources = { ...next.resources, equipment: !next.resources.equipment };
        break;
      case "customPins":
        next.customPins = !next.customPins;
        break;
      case "mkHazard":
        next.mapMarkers = { ...next.mapMarkers, hazard: !next.mapMarkers.hazard };
        break;
      case "mkWater":
        next.mapMarkers = { ...next.mapMarkers, water: !next.mapMarkers.water };
        break;
      case "mkCache":
        next.mapMarkers = { ...next.mapMarkers, cache: !next.mapMarkers.cache };
        break;
      case "mkMed":
        next.mapMarkers = { ...next.mapMarkers, med: !next.mapMarkers.med };
        break;
      case "libPois":
        setLayerFilters((p) => ({ ...p, pois: !p.pois }));
        return;
      case "drawings":
        next.drawings = !next.drawings;
        break;
      case "blockages":
        next.routeBlockages = !next.routeBlockages;
        break;
      case "dangerPins":
        next.dangerPins = !next.dangerPins;
        break;
      default:
        return;
    }
    onTacticalLayerVisibilityChange(next);
  }

  function isItemOn(id: string): boolean {
    switch (id) {
      case "mesh":
        return v.meshNodes;
      case "peers":
        return v.peerPositions;
      case "nodes":
        return v.network.nodes;
      case "cov":
        return v.network.coverageCircles;
      case "links":
        return v.network.links;
      case "power":
        return v.resources.power;
      case "water":
        return v.resources.water;
      case "food":
        return v.resources.food;
      case "equipment":
        return v.resources.equipment;
      case "customPins":
        return v.customPins;
      case "mkHazard":
        return v.mapMarkers.hazard;
      case "mkWater":
        return v.mapMarkers.water;
      case "mkCache":
        return v.mapMarkers.cache;
      case "mkMed":
        return v.mapMarkers.med;
      case "libPois":
        return layerFilters.pois;
      case "drawings":
        return v.drawings;
      case "blockages":
        return v.routeBlockages;
      case "dangerPins":
        return v.dangerPins;
      default:
        return false;
    }
  }

  async function handleExport() {
    try {
      setExporting(true);
      setStatusMessage("");
      const path = await invoke<string>("export_tactical_data");
      setStatusMessage(`Exported tactical data to ${path}`);
    } catch (error: unknown) {
      setStatusMessage(error != null ? String(error) : "Tactical export failed.");
    } finally {
      setExporting(false);
    }
  }

  async function handleImport() {
    try {
      setImporting(true);
      setStatusMessage("");
      const chosen = await openNativeFileDialog({
        multiple: false,
        filters: [{ name: "Athena Tactical File", extensions: ["athena-tactical", "json"] }],
      });
      const selectedPath = typeof chosen === "string" ? chosen : Array.isArray(chosen) ? chosen[0] : null;
      const result = await invoke<TacticalImportResult>("import_tactical_data", {
        sourcePath: selectedPath,
      });
      setStatusMessage(
        `Imported ${result.imported_points_of_interest} points and ${result.imported_mesh_node_locations} nodes.`,
      );
      onTacticalDataImported?.();
    } catch (error: unknown) {
      setStatusMessage(error != null ? String(error) : "Tactical import failed.");
    } finally {
      setImporting(false);
    }
  }

  if (!panelOpen) return null;

  return (
    <div
      className="map-overlays-popout glass-panel"
      role="dialog"
      aria-label="Map overlays"
      style={{ pointerEvents: "auto" }}
    >
      <div className="map-panel__head">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 8 }}>
          <strong>Map overlays</strong>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>

        <p style={{ fontSize: 12, opacity: 0.85, margin: "6px 0 10px" }}>
          Choose a category to focus the map, then toggle icon families. Disabled tiles do not apply in the current
          category.
        </p>

        <div style={{ display: "flex", gap: 8, marginBottom: 10, flexWrap: "wrap" }}>
          <button
            type="button"
            className="secondary small-btn"
            onClick={() => {
              if (allOn) {
                onTacticalLayerVisibilityChange(allLayersOff());
              } else {
                onTacticalLayerVisibilityChange(defaultTacticalLayerVisibilityState);
                setLayerFilters((p) => ({ ...p, pois: true }));
              }
            }}
          >
            {allOn ? "Hide all" : "Show all"}
          </button>
          <button
            type="button"
            className="secondary small-btn"
            onClick={() => {
              onTacticalLayerVisibilityChange(mergeTacticalLayerVisibility({}));
              setLayerFilters((p) => ({ ...p, pois: true, roads: true, rivers: true }));
              setStatusMessage("Overlays reset to default visibility.");
            }}
          >
            Reset to defaults
          </button>
          <button type="button" className="secondary small-btn" onClick={() => onOpenBlockagesManager()}>
            Manage blockages…
          </button>
        </div>

        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 12 }}>
          {(
            [
              { id: "all" as const, label: "All" },
              { id: "tactical" as const, label: "Tactical" },
              { id: "resources" as const, label: "Resources" },
              { id: "obstructions" as const, label: "Obstructions / dangers" },
            ] as const
          ).map((c) => {
            const active = v.overlayCategory === c.id;
            return (
              <button
                key={c.id}
                type="button"
                className="secondary small-btn"
                style={{
                  border: active ? "1px solid color-mix(in srgb, var(--primary) 60%, transparent)" : undefined,
                  background: active ? "color-mix(in srgb, var(--primary) 16%, transparent)" : undefined,
                }}
                onClick={() => onTacticalLayerVisibilityChange(mergeTacticalLayerVisibility({ overlayCategory: c.id }))}
              >
                {c.label}
              </button>
            );
          })}
        </div>
      </div>

      <div className="map-panel__body">
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
            gap: 8,
          }}
        >
          {gridItems.map((item) => {
            const enabled = itemEnabled(item);
            const on = isItemOn(item.id);
            return (
              <button
                key={item.id}
                type="button"
                title={item.label}
                aria-label={item.label}
                disabled={!enabled}
                onClick={() => toggleGridItem(item.id)}
                style={{
                  borderRadius: 12,
                  border: on && enabled ? "1px solid color-mix(in srgb, var(--primary) 55%, transparent)" : "1px solid var(--border)",
                  background: on && enabled ? "color-mix(in srgb, var(--primary) 18%, transparent)" : "color-mix(in srgb, var(--surface-solid) 70%, transparent)",
                  color: enabled ? "var(--text)" : "color-mix(in srgb, var(--text) 35%, transparent)",
                  minHeight: 56,
                  display: "grid",
                  placeItems: "center",
                  cursor: enabled ? "pointer" : "not-allowed",
                  opacity: enabled ? 1 : 0.45,
                }}
              >
                {item.icon}
              </button>
            );
          })}
        </div>

        <div style={{ marginTop: 12, fontSize: 11, opacity: 0.75 }}>
          Marker legend:{" "}
          {MAP_MARKER_CATEGORIES.map((c) => (
            <span key={c.id} style={{ marginRight: 8 }}>
              {c.label}
            </span>
          ))}
        </div>

        <div style={{ display: "flex", gap: 8, marginTop: 12, flexWrap: "wrap" }}>
          <button type="button" className="secondary small-btn" disabled={exporting} onClick={() => void handleExport()}>
            <DatabaseBackup size={14} style={{ marginRight: 6, verticalAlign: "middle" }} />
            {exporting ? "Exporting…" : "Export tactical"}
          </button>
          <button type="button" className="secondary small-btn" disabled={importing} onClick={() => void handleImport()}>
            <Upload size={14} style={{ marginRight: 6, verticalAlign: "middle" }} />
            {importing ? "Importing…" : "Import tactical"}
          </button>
        </div>

        {statusMessage ? (
          <p style={{ marginTop: 10, fontSize: 12, opacity: 0.9 }} role="status">
            {statusMessage}
          </p>
        ) : null}
      </div>
    </div>
  );
}
