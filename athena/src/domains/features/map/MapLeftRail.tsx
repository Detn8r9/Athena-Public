import { Bell, MapPinned, Layers, Route, LayoutGrid, Hexagon } from "lucide-react";
import { SoftToast } from "../../../ui/components/SoftToast";
import { useState } from "react";

export function MapLeftRail({
  layersOpen,
  onLayersOpenChange,
  overlaysOpen,
  onOverlaysOpenChange,
  savedOpen,
  onSavedOpenChange,
  territoriesOpen,
  onTerritoriesOpenChange,
  onToggleRoute,
  directionsOpen,
  hasSatelliteRaster: _hasSatelliteRaster,
  onOpenApprovalInbox,
  aiPendingCount = 0,
  showApprovalInbox = false,
}: {
  layersOpen: boolean;
  onLayersOpenChange: (v: boolean) => void;
  overlaysOpen: boolean;
  onOverlaysOpenChange: (v: boolean) => void;
  savedOpen: boolean;
  onSavedOpenChange: (v: boolean) => void;
  territoriesOpen: boolean;
  onTerritoriesOpenChange: (v: boolean) => void;
  onToggleRoute: () => void;
  directionsOpen: boolean;
  hasSatelliteRaster: boolean;
  onOpenApprovalInbox?: () => void;
  aiPendingCount?: number;
  showApprovalInbox?: boolean;
}) {
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);

  return (
    <div className="map-left-rail glass-panel" style={{ pointerEvents: "auto", position: "relative", zIndex: 1400 }}>
      <div className="map-left-rail__stack">
        <button
          type="button"
          className={`map-left-rail__btn${layersOpen ? " is-active" : ""}`}
          onClick={() => {
            if (!layersOpen) onOverlaysOpenChange(false);
            onTerritoriesOpenChange(false);
            onLayersOpenChange(!layersOpen);
          }}
          title="Map layers"
          aria-label="Map layers"
        >
          <Layers size={20} />
          <span>Layers</span>
        </button>
        <button
          type="button"
          className={`map-left-rail__btn${overlaysOpen ? " is-active" : ""}`}
          onClick={() => {
            if (!overlaysOpen) onLayersOpenChange(false);
            onTerritoriesOpenChange(false);
            onOverlaysOpenChange(!overlaysOpen);
          }}
          title="Map overlays (tactical, resources, hazards)"
          aria-label="Map overlays"
        >
          <LayoutGrid size={20} />
          <span>Overlays</span>
        </button>
        <button
          type="button"
          className={`map-left-rail__btn${directionsOpen ? " is-active" : ""}`}
          onClick={onToggleRoute}
          title="Directions"
          aria-label="Open directions"
        >
          <Route size={20} />
          <span>Directions</span>
        </button>
        {showApprovalInbox && onOpenApprovalInbox ? (
          <button
            type="button"
            className="map-left-rail__btn"
            onClick={onOpenApprovalInbox}
            title="AI approval inbox (rank 4+)"
            aria-label="Open approval inbox"
          >
            <div style={{ position: "relative", display: "inline-flex" }}>
              <Bell size={20} />
              {aiPendingCount > 0 ? (
                <span className="map-approval-badge">
                  {aiPendingCount > 9 ? "9+" : aiPendingCount}
                </span>
              ) : null}
            </div>
            <span>Inbox</span>
          </button>
        ) : null}
        <button
          type="button"
          className={`map-left-rail__btn${territoriesOpen ? " is-active" : ""}`}
          onClick={() => onTerritoriesOpenChange(!territoriesOpen)}
          title="Territories"
          aria-label="Territories"
        >
          <Hexagon size={20} />
          <span>Territories</span>
        </button>
        <button
          type="button"
          className={`map-left-rail__btn${savedOpen ? " is-active" : ""}`}
          onClick={() => onSavedOpenChange(!savedOpen)}
          title="Saved places"
          aria-label="Saved places"
        >
          <MapPinned size={20} />
          <span>Saved</span>
        </button>
      </div>
      <SoftToast
        open={toast != null}
        tone={toast?.tone ?? "warning"}
        message={toast?.message ?? ""}
        onClose={() => setToast(null)}
      />
    </div>
  );
}
