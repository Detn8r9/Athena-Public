import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronLeft,
  ChevronRight,
  CircleDot,
  DatabaseBackup,
  Link2,
  MapPin,
  Network,
  Package,
  Power,
  RadioTower,
  Upload,
  Utensils,
  Waves,
} from "lucide-react";

export type MapOverlayCategory = "all" | "tactical" | "resources" | "obstructions";

export type TacticalLayerVisibilityState = {
  /**
   * Legacy field kept for persisted configs; map rendering now uses `overlayCategory` + per-layer toggles.
   */
  activeFamily: "network" | "resources";
  network: {
    nodes: boolean;
    coverageCircles: boolean;
    links: boolean;
  };
  resources: {
    power: boolean;
    water: boolean;
    food: boolean;
    equipment: boolean;
  };
  /** Right-click / custom `@icon:` pins that are not the core preset icons (Home, Zap, Droplets, Apple). */
  customPins: boolean;

  /** High-level map overlay filter (solo-category view). */
  overlayCategory: MapOverlayCategory;
  meshNodes: boolean;
  peerPositions: boolean;
  /** User map markers (`get_map_markers`) by normalized category. */
  mapMarkers: {
    hazard: boolean;
    water: boolean;
    cache: boolean;
    med: boolean;
  };
  /** User drawings overlay. */
  drawings: boolean;
  /** Route blockages drawn on the map. */
  routeBlockages: boolean;
  /** Tactical points whose notes look like hazards / obstructions. */
  dangerPins: boolean;
};

export const defaultTacticalLayerVisibilityState: TacticalLayerVisibilityState = {
  activeFamily: "resources",
  network: {
    nodes: true,
    coverageCircles: true,
    links: true,
  },
  resources: {
    power: true,
    water: true,
    food: true,
    equipment: true,
  },
  customPins: true,
  overlayCategory: "all",
  meshNodes: true,
  peerPositions: true,
  mapMarkers: {
    hazard: true,
    water: true,
    cache: true,
    med: true,
  },
  drawings: true,
  routeBlockages: true,
  dangerPins: true,
};

type TacticalImportResult = {
  imported_points_of_interest: number;
  imported_mesh_node_locations: number;
  source_path: string;
};

export interface TacticalSidebarProps {
  initialVisibilityState?: TacticalLayerVisibilityState;
  onVisibilityStateChange?: (visibilityState: TacticalLayerVisibilityState) => void;
  onTacticalDataImported?: () => void;
}

const COLLAPSE_AFTER_IDLE_MS = 5000;

export function mergeTacticalLayerVisibility(
  incoming?: Partial<TacticalLayerVisibilityState>,
): TacticalLayerVisibilityState {
  const d = defaultTacticalLayerVisibilityState;
  return {
    activeFamily: incoming?.activeFamily ?? d.activeFamily,
    network: {
      nodes: incoming?.network?.nodes ?? d.network.nodes,
      coverageCircles: incoming?.network?.coverageCircles ?? d.network.coverageCircles,
      links: incoming?.network?.links ?? d.network.links,
    },
    resources: {
      power: incoming?.resources?.power ?? d.resources.power,
      water: incoming?.resources?.water ?? d.resources.water,
      food: incoming?.resources?.food ?? d.resources.food,
      equipment: incoming?.resources?.equipment ?? d.resources.equipment,
    },
    customPins: incoming?.customPins ?? d.customPins,
    overlayCategory: incoming?.overlayCategory ?? d.overlayCategory,
    meshNodes: incoming?.meshNodes ?? d.meshNodes,
    peerPositions: incoming?.peerPositions ?? d.peerPositions,
    mapMarkers: {
      hazard: incoming?.mapMarkers?.hazard ?? d.mapMarkers.hazard,
      water: incoming?.mapMarkers?.water ?? d.mapMarkers.water,
      cache: incoming?.mapMarkers?.cache ?? d.mapMarkers.cache,
      med: incoming?.mapMarkers?.med ?? d.mapMarkers.med,
    },
    drawings: incoming?.drawings ?? d.drawings,
    routeBlockages: incoming?.routeBlockages ?? d.routeBlockages,
    dangerPins: incoming?.dangerPins ?? d.dangerPins,
  };
}

function normalizeInitialState(incoming?: TacticalLayerVisibilityState): TacticalLayerVisibilityState {
  return mergeTacticalLayerVisibility(incoming);
}

export function TacticalSidebar({
  initialVisibilityState,
  onVisibilityStateChange,
  onTacticalDataImported,
}: TacticalSidebarProps) {
  const [expanded, setExpanded] = useState(false);
  const [visibilityState, setVisibilityState] = useState<TacticalLayerVisibilityState>(() =>
    normalizeInitialState(initialVisibilityState),
  );
  const [statusMessage, setStatusMessage] = useState<string>("");
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const collapseTimerRef = useRef<number | null>(null);

  useEffect(() => {
    onVisibilityStateChange?.(visibilityState);
  }, [visibilityState, onVisibilityStateChange]);

  const clearCollapseTimer = () => {
    if (collapseTimerRef.current !== null) {
      window.clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
  };

  const scheduleAutoCollapse = () => {
    clearCollapseTimer();
    collapseTimerRef.current = window.setTimeout(() => {
      setExpanded(false);
    }, COLLAPSE_AFTER_IDLE_MS);
  };

  useEffect(() => {
    if (!expanded) {
      clearCollapseTimer();
      return;
    }
    scheduleAutoCollapse();
    return () => {
      clearCollapseTimer();
    };
  }, [expanded]);

  const toggleNetworkLayer = (
    key: keyof TacticalLayerVisibilityState["network"],
    nextValue: boolean,
  ) => {
    setVisibilityState((previous) => ({
      ...previous,
      activeFamily: "network",
      network: {
        ...previous.network,
        [key]: nextValue,
      },
    }));
  };

  const toggleResourceLayer = (
    key: keyof TacticalLayerVisibilityState["resources"],
    nextValue: boolean,
  ) => {
    setVisibilityState((previous) => ({
      ...previous,
      activeFamily: "resources",
      resources: {
        ...previous.resources,
        [key]: nextValue,
      },
    }));
  };

  const toggleCustomPinsLayer = (nextValue: boolean) => {
    setVisibilityState((previous) => ({
      ...previous,
      activeFamily: "resources",
      customPins: nextValue,
    }));
  };

  const setActiveFamily = (family: "network" | "resources") => {
    setVisibilityState((previous) => ({
      ...previous,
      activeFamily: family,
    }));
  };

  const handleExport = async () => {
    try {
      setExporting(true);
      setStatusMessage("");
      const path = await invoke<string>("export_tactical_data");
      setStatusMessage(`Exported tactical data to ${path}`);
    } catch (error: unknown) {
      setStatusMessage(error != null ? String(error) : "Tactical export failed.");
    } finally {
      setExporting(false);
      scheduleAutoCollapse();
    }
  };

  const handleImport = async () => {
    try {
      setImporting(true);
      setStatusMessage("");
      const chosen = await open({
        multiple: false,
        filters: [{ name: "Athena Tactical File", extensions: ["athena-tactical", "json"] }],
      });
      const selectedPath =
        typeof chosen === "string" ? chosen : Array.isArray(chosen) ? chosen[0] : null;
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
      scheduleAutoCollapse();
    }
  };

  return (
    <aside
      className={`tactical-sidebar glass-panel${expanded ? " is-expanded" : ""}`}
      onMouseEnter={clearCollapseTimer}
      onMouseLeave={() => {
        if (expanded) {
          scheduleAutoCollapse();
        }
      }}
      aria-label="Tactical Layer Manager"
    >
      <div className="tactical-sidebar-rail">
        <button
          type="button"
          className="tactical-sidebar-icon-btn"
          onClick={() => {
            setExpanded((previous) => !previous);
            if (!expanded) {
              scheduleAutoCollapse();
            }
          }}
          aria-label={expanded ? "Collapse tactical sidebar" : "Expand tactical sidebar"}
          title={expanded ? "Collapse tactical sidebar" : "Expand tactical sidebar"}
        >
          {expanded ? <ChevronLeft size={18} /> : <ChevronRight size={18} />}
        </button>
        <button
          type="button"
          className={`tactical-sidebar-icon-btn${
            visibilityState.activeFamily === "network" ? " is-active" : ""
          }`}
          onClick={() => {
            setExpanded(true);
            setActiveFamily("network");
            scheduleAutoCollapse();
          }}
          aria-label="Network layer controls"
          title="Network layer controls"
        >
          <Network size={17} />
        </button>
        <button
          type="button"
          className={`tactical-sidebar-icon-btn${
            visibilityState.activeFamily === "resources" ? " is-active" : ""
          }`}
          onClick={() => {
            setExpanded(true);
            setActiveFamily("resources");
            scheduleAutoCollapse();
          }}
          aria-label="Resource layer controls"
          title="Resource layer controls"
        >
          <Package size={17} />
        </button>
        <button
          type="button"
          className="tactical-sidebar-icon-btn"
          onClick={() => void handleExport()}
          disabled={exporting}
          aria-label="Export tactical data"
          title="Export tactical data"
        >
          <DatabaseBackup size={17} />
        </button>
        <button
          type="button"
          className="tactical-sidebar-icon-btn"
          onClick={() => void handleImport()}
          disabled={importing}
          aria-label="Import tactical data"
          title="Import tactical data"
        >
          <Upload size={17} />
        </button>
      </div>

      <div className="tactical-sidebar-pane">
        <h3 className="tactical-sidebar-title">Tactical Layer Manager</h3>
        <p className="tactical-sidebar-subtitle">
          Sidebar auto-collapses after inactivity to preserve map viewport space.
        </p>

        <div className="tactical-sidebar-family-row">
          <button
            type="button"
            className={`tactical-sidebar-pill-btn${
              visibilityState.activeFamily === "network" ? " is-active" : ""
            }`}
            onClick={() => setActiveFamily("network")}
          >
            <Network size={14} className="tactical-sidebar-pill-icon" />
            Network
          </button>
          <button
            type="button"
            className={`tactical-sidebar-pill-btn${
              visibilityState.activeFamily === "resources" ? " is-active" : ""
            }`}
            onClick={() => setActiveFamily("resources")}
          >
            <Package size={14} className="tactical-sidebar-pill-icon" />
            Resources
          </button>
        </div>

        {visibilityState.activeFamily === "network" ? (
          <section>
            <div className="tactical-sidebar-section-label">Network</div>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.network.nodes}
                onChange={(event) => toggleNetworkLayer("nodes", event.target.checked)}
              />
              <RadioTower size={15} />
              Nodes
            </label>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.network.coverageCircles}
                onChange={(event) => toggleNetworkLayer("coverageCircles", event.target.checked)}
              />
              <CircleDot size={15} />
              Coverage Circles
            </label>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.network.links}
                onChange={(event) => toggleNetworkLayer("links", event.target.checked)}
              />
              <Link2 size={15} />
              Links
            </label>
          </section>
        ) : (
          <section>
            <div className="tactical-sidebar-section-label">Resources</div>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.resources.power}
                onChange={(event) => toggleResourceLayer("power", event.target.checked)}
              />
              <Power size={15} />
              Power
            </label>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.resources.water}
                onChange={(event) => toggleResourceLayer("water", event.target.checked)}
              />
              <Waves size={15} />
              Water
            </label>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.resources.food}
                onChange={(event) => toggleResourceLayer("food", event.target.checked)}
              />
              <Utensils size={15} />
              Food
            </label>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.resources.equipment}
                onChange={(event) => toggleResourceLayer("equipment", event.target.checked)}
              />
              <Package size={15} />
              Equipment
            </label>
            <div className="tactical-sidebar-section-label tactical-sidebar-section-label-spaced">
              Custom pins
            </div>
            <label className="tactical-sidebar-row">
              <input
                type="checkbox"
                checked={visibilityState.customPins}
                onChange={(event) => toggleCustomPinsLayer(event.target.checked)}
              />
              <MapPin size={15} />
              Custom Pins
            </label>
          </section>
        )}

        <div className="tactical-sidebar-actions">
          <button
            type="button"
            className="tactical-sidebar-pill-btn"
            disabled={exporting}
            onClick={() => void handleExport()}
          >
            <DatabaseBackup size={14} className="tactical-sidebar-pill-icon" />
            {exporting ? "Exporting" : "Export"}
          </button>
          <button
            type="button"
            className="tactical-sidebar-pill-btn"
            disabled={importing}
            onClick={() => void handleImport()}
          >
            <Upload size={14} className="tactical-sidebar-pill-icon" />
            {importing ? "Importing" : "Import"}
          </button>
        </div>

        {statusMessage.length > 0 && (
          <p className="tactical-sidebar-status" role="status">
            {statusMessage}
          </p>
        )}
      </div>
    </aside>
  );
}
