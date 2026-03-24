import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
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

export type TacticalLayerVisibilityState = {
  /**
   * Network vs Resources are mutually exclusive on the map: switching families hides the other
   * family’s overlays so the view stays readable.
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

const panelStyle: CSSProperties = {
  position: "absolute",
  top: "calc(4.5rem + env(safe-area-inset-top, 0px))",
  left: "calc(1rem + env(safe-area-inset-left, 0px))",
  zIndex: 1200,
  display: "flex",
  alignItems: "stretch",
  borderRadius: "16px",
  border: "1px solid rgba(255, 255, 255, 0.1)",
  background: "rgba(15, 23, 42, 0.75)",
  backdropFilter: "blur(12px)",
  WebkitBackdropFilter: "blur(12px)",
  boxShadow: "0 16px 40px rgba(2, 6, 23, 0.4)",
  overflow: "hidden",
};

const iconRailStyle: CSSProperties = {
  width: "48px",
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
  padding: "0.35rem",
  borderRight: "1px solid rgba(255, 255, 255, 0.1)",
  background: "rgba(15, 23, 42, 0.35)",
};

const iconButtonStyle: CSSProperties = {
  width: "38px",
  height: "38px",
  borderRadius: "12px",
  border: "1px solid rgba(255, 255, 255, 0.1)",
  background: "rgba(30, 41, 59, 0.45)",
  color: "#e2e8f0",
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  cursor: "pointer",
  padding: 0,
};

const sectionLabelStyle: CSSProperties = {
  margin: "0.2rem 0 0.35rem",
  fontSize: "0.74rem",
  letterSpacing: "0.06em",
  textTransform: "uppercase",
  fontWeight: 700,
  color: "#94a3b8",
};

const rowLabelStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  fontSize: "0.84rem",
  color: "#e2e8f0",
  marginBottom: "0.35rem",
};

function normalizeInitialState(
  incoming?: TacticalLayerVisibilityState,
): TacticalLayerVisibilityState {
  return {
    activeFamily: incoming?.activeFamily ?? defaultTacticalLayerVisibilityState.activeFamily,
    network: {
      nodes: incoming?.network.nodes ?? defaultTacticalLayerVisibilityState.network.nodes,
      coverageCircles:
        incoming?.network.coverageCircles ??
        defaultTacticalLayerVisibilityState.network.coverageCircles,
      links: incoming?.network.links ?? defaultTacticalLayerVisibilityState.network.links,
    },
    resources: {
      power: incoming?.resources.power ?? defaultTacticalLayerVisibilityState.resources.power,
      water: incoming?.resources.water ?? defaultTacticalLayerVisibilityState.resources.water,
      food: incoming?.resources.food ?? defaultTacticalLayerVisibilityState.resources.food,
      equipment:
        incoming?.resources.equipment ?? defaultTacticalLayerVisibilityState.resources.equipment,
    },
    customPins: incoming?.customPins ?? defaultTacticalLayerVisibilityState.customPins,
  };
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

  const expandedPaneStyle = useMemo<CSSProperties>(
    () => ({
      width: expanded ? "250px" : "0",
      overflow: "hidden",
      transition: "width 220ms ease",
      padding: expanded ? "0.6rem 0.65rem 0.65rem" : "0",
      display: "flex",
      flexDirection: "column",
      gap: "0.4rem",
    }),
    [expanded],
  );

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
      style={panelStyle}
      onMouseEnter={clearCollapseTimer}
      onMouseLeave={() => {
        if (expanded) {
          scheduleAutoCollapse();
        }
      }}
      aria-label="Tactical Layer Manager"
    >
      <div style={iconRailStyle}>
        <button
          type="button"
          style={iconButtonStyle}
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
          style={{
            ...iconButtonStyle,
            borderColor:
              visibilityState.activeFamily === "network"
                ? "rgba(56, 189, 248, 0.8)"
                : iconButtonStyle.borderColor,
          }}
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
          style={{
            ...iconButtonStyle,
            borderColor:
              visibilityState.activeFamily === "resources"
                ? "rgba(56, 189, 248, 0.8)"
                : iconButtonStyle.borderColor,
          }}
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
          style={iconButtonStyle}
          onClick={() => void handleExport()}
          disabled={exporting}
          aria-label="Export tactical data"
          title="Export tactical data"
        >
          <DatabaseBackup size={17} />
        </button>
        <button
          type="button"
          style={iconButtonStyle}
          onClick={() => void handleImport()}
          disabled={importing}
          aria-label="Import tactical data"
          title="Import tactical data"
        >
          <Upload size={17} />
        </button>
      </div>

      <div style={expandedPaneStyle}>
        <h3
          style={{
            margin: 0,
            fontSize: "0.95rem",
            fontWeight: 700,
            color: "#f8fafc",
          }}
        >
          Tactical Layer Manager
        </h3>
        <p
          style={{
            margin: 0,
            fontSize: "0.76rem",
            lineHeight: 1.45,
            color: "#94a3b8",
          }}
        >
          Sidebar auto-collapses after inactivity to preserve map viewport space.
        </p>

        <div style={{ display: "flex", gap: "0.4rem", marginTop: "0.15rem" }}>
          <button
            type="button"
            style={{
              ...iconButtonStyle,
              width: "auto",
              height: "32px",
              padding: "0 0.65rem",
              borderColor:
                visibilityState.activeFamily === "network"
                  ? "rgba(56, 189, 248, 0.8)"
                  : iconButtonStyle.borderColor,
            }}
            onClick={() => setActiveFamily("network")}
          >
            <Network size={14} style={{ marginRight: "0.35rem" }} />
            Network
          </button>
          <button
            type="button"
            style={{
              ...iconButtonStyle,
              width: "auto",
              height: "32px",
              padding: "0 0.65rem",
              borderColor:
                visibilityState.activeFamily === "resources"
                  ? "rgba(56, 189, 248, 0.8)"
                  : iconButtonStyle.borderColor,
            }}
            onClick={() => setActiveFamily("resources")}
          >
            <Package size={14} style={{ marginRight: "0.35rem" }} />
            Resources
          </button>
        </div>

        {visibilityState.activeFamily === "network" ? (
          <section>
            <div style={sectionLabelStyle}>Network</div>
            <label style={rowLabelStyle}>
              <input
                type="checkbox"
                checked={visibilityState.network.nodes}
                onChange={(event) => toggleNetworkLayer("nodes", event.target.checked)}
              />
              <RadioTower size={15} />
              Nodes
            </label>
            <label style={rowLabelStyle}>
              <input
                type="checkbox"
                checked={visibilityState.network.coverageCircles}
                onChange={(event) => toggleNetworkLayer("coverageCircles", event.target.checked)}
              />
              <CircleDot size={15} />
              Coverage Circles
            </label>
            <label style={rowLabelStyle}>
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
            <div style={sectionLabelStyle}>Resources</div>
            <label style={rowLabelStyle}>
              <input
                type="checkbox"
                checked={visibilityState.resources.power}
                onChange={(event) => toggleResourceLayer("power", event.target.checked)}
              />
              <Power size={15} />
              Power
            </label>
            <label style={rowLabelStyle}>
              <input
                type="checkbox"
                checked={visibilityState.resources.water}
                onChange={(event) => toggleResourceLayer("water", event.target.checked)}
              />
              <Waves size={15} />
              Water
            </label>
            <label style={rowLabelStyle}>
              <input
                type="checkbox"
                checked={visibilityState.resources.food}
                onChange={(event) => toggleResourceLayer("food", event.target.checked)}
              />
              <Utensils size={15} />
              Food
            </label>
            <label style={rowLabelStyle}>
              <input
                type="checkbox"
                checked={visibilityState.resources.equipment}
                onChange={(event) => toggleResourceLayer("equipment", event.target.checked)}
              />
              <Package size={15} />
              Equipment
            </label>
            <div style={{ ...sectionLabelStyle, marginTop: "0.55rem" }}>Custom pins</div>
            <label style={rowLabelStyle}>
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

        <div style={{ display: "flex", gap: "0.45rem", marginTop: "0.2rem" }}>
          <button
            type="button"
            style={{
              ...iconButtonStyle,
              width: "auto",
              height: "32px",
              padding: "0 0.65rem",
              opacity: exporting ? 0.7 : 1,
            }}
            disabled={exporting}
            onClick={() => void handleExport()}
          >
            <DatabaseBackup size={14} style={{ marginRight: "0.35rem" }} />
            {exporting ? "Exporting" : "Export"}
          </button>
          <button
            type="button"
            style={{
              ...iconButtonStyle,
              width: "auto",
              height: "32px",
              padding: "0 0.65rem",
              opacity: importing ? 0.7 : 1,
            }}
            disabled={importing}
            onClick={() => void handleImport()}
          >
            <Upload size={14} style={{ marginRight: "0.35rem" }} />
            {importing ? "Importing" : "Import"}
          </button>
        </div>

        {statusMessage.length > 0 && (
          <p
            style={{
              margin: "0.25rem 0 0",
              fontSize: "0.74rem",
              lineHeight: 1.35,
              color: "#cbd5e1",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
            role="status"
          >
            {statusMessage}
          </p>
        )}
      </div>
    </aside>
  );
}
