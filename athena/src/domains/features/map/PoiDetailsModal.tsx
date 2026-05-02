import { useCallback, useEffect, useMemo, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  Antenna,
  Bluetooth,
  MapPin,
  Network,
  Radio,
  RadioTower,
  Satellite,
  Smartphone,
  Wifi,
  Home,
} from "lucide-react";
import { getPoiLucideIcon, POI_CUSTOM_LUCIDE_GRID, POI_PRESET_ENTRIES } from "./tacticalLucideIconRegistry";

export interface TacticalPinSavePayload {
  name: string;
  description: string;
  /** Empty string → backend defaults to standard `MapPin`. */
  lucideIconName: string;
  pinLayer: "resource" | "network";
  lat: number;
  lng: number;
  /** Network pins only: stored in notes as @range_m:<meters> via description tail. */
  coverageRadiusMeters?: number | null;
}

/** Meshtastic / RF / IP comms — stored with `@network` when Network tab is active. */
const NETWORK_PRESET_ENTRIES: readonly { label: string; lucideIconName: string; Icon: LucideIcon }[] = [
  { label: "Home", lucideIconName: "Home", Icon: Home },
  { label: "Mesh / net", lucideIconName: "Network", Icon: Network },
  { label: "Radio tower", lucideIconName: "RadioTower", Icon: RadioTower },
  { label: "Radio", lucideIconName: "Radio", Icon: Radio },
  { label: "Wi‑Fi", lucideIconName: "Wifi", Icon: Wifi },
  { label: "Phone", lucideIconName: "Smartphone", Icon: Smartphone },
  { label: "Bluetooth", lucideIconName: "Bluetooth", Icon: Bluetooth },
  { label: "Satellite", lucideIconName: "Satellite", Icon: Satellite },
  { label: "Antenna", lucideIconName: "Antenna", Icon: Antenna },
];

export interface PoiDetailsModalProps {
  open: boolean;
  onClose: () => void;
  latLng: { lat: number; lng: number } | null;
  onSave: (payload: TacticalPinSavePayload) => void | Promise<void>;
}

type TabId = "resources" | "network" | "custom";

export function PoiDetailsModal({ open, onClose, latLng, onSave }: PoiDetailsModalProps) {
  const [activeTab, setActiveTab] = useState<TabId>("resources");
  const [pinName, setPinName] = useState("");
  const [description, setDescription] = useState("");
  /** Empty = default standard map pin (MapPin) on save */
  const [selectedLucideIconName, setSelectedLucideIconName] = useState<string>("");
  const [iconSearch, setIconSearch] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [networkCoverageRadiusM, setNetworkCoverageRadiusM] = useState("");

  useEffect(() => {
    if (!open) {
      setActiveTab("resources");
      setPinName("");
      setDescription("");
      setSelectedLucideIconName("");
      setIconSearch("");
      setSaveError(null);
      setSaving(false);
      setNetworkCoverageRadiusM("");
    }
  }, [open]);

  useEffect(() => {
    if (open && latLng) {
      setPinName("");
      setDescription("");
      setSelectedLucideIconName("");
      setIconSearch("");
      setSaveError(null);
      setNetworkCoverageRadiusM("");
    }
  }, [open, latLng?.lat, latLng?.lng]);

  const filteredCustomIcons = useMemo(() => {
    const q = iconSearch.trim().toLowerCase();
    if (!q) return [...POI_CUSTOM_LUCIDE_GRID];
    return POI_CUSTOM_LUCIDE_GRID.filter((entry) => entry.name.toLowerCase().includes(q));
  }, [iconSearch]);

  const displayIconName = selectedLucideIconName.trim().length > 0 ? selectedLucideIconName.trim() : "MapPin";
  const DisplayIcon = getPoiLucideIcon(displayIconName);

  const canSave = pinName.trim().length > 0 && latLng !== null && !saving;

  const handleIconPick = useCallback((lucideIconName: string) => {
    setSelectedLucideIconName(lucideIconName);
    setSaveError(null);
  }, []);

  const handleSave = useCallback(async () => {
    if (!latLng) return;
    const name = pinName.trim();
    if (!name) return;
    setSaveError(null);
    setSaving(true);
    try {
      let coverageRadiusMeters: number | null = null;
      if (activeTab === "network") {
        const raw = networkCoverageRadiusM.trim();
        if (raw !== "") {
          const n = Number.parseFloat(raw);
          if (Number.isFinite(n) && n > 0) coverageRadiusMeters = n;
        }
      }
      await onSave({
        name,
        description: description.trim(),
        lucideIconName: selectedLucideIconName.trim(),
        pinLayer: activeTab === "network" ? "network" : "resource",
        lat: latLng.lat,
        lng: latLng.lng,
        coverageRadiusMeters,
      });
      onClose();
    } catch (e) {
      setSaveError(e != null ? String(e) : "Could not save pin.");
    } finally {
      setSaving(false);
    }
  }, [activeTab, description, latLng, networkCoverageRadiusM, onClose, onSave, pinName, selectedLucideIconName]);

  if (!open || latLng === null) {
    return null;
  }

  const resourcesGrid = (
    <div className="poi-preset-grid poi-preset-grid--dense">
      {POI_PRESET_ENTRIES.map(({ label, lucideIconName, Icon }) => {
        const selected = selectedLucideIconName === lucideIconName;
        return (
          <button
            key={lucideIconName}
            type="button"
            className={"poi-preset-btn" + (selected ? " poi-preset-btn-selected" : "")}
            onClick={() => handleIconPick(lucideIconName)}
            aria-pressed={selected}
          >
            <Icon className="poi-preset-btn-icon" aria-hidden />
            <span className="poi-preset-btn-label">{label}</span>
          </button>
        );
      })}
    </div>
  );

  const networkGrid = (
    <div className="poi-preset-grid poi-preset-grid--dense">
      {NETWORK_PRESET_ENTRIES.map(({ label, lucideIconName, Icon }) => {
        const selected = selectedLucideIconName === lucideIconName;
        return (
          <button
            key={lucideIconName}
            type="button"
            className={"poi-preset-btn" + (selected ? " poi-preset-btn-selected" : "")}
            onClick={() => handleIconPick(lucideIconName)}
            aria-pressed={selected}
          >
            <Icon className="poi-preset-btn-icon" aria-hidden />
            <span className="poi-preset-btn-label">{label}</span>
          </button>
        );
      })}
    </div>
  );

  const customPanel = (
    <div className="poi-custom-tab">
      <input
        id="poi-icon-search"
        type="search"
        className="poi-icon-search-input"
        value={iconSearch}
        onChange={(e) => setIconSearch(e.target.value)}
        placeholder="Filter icons by name…"
        autoComplete="off"
        spellCheck={false}
        aria-label="Search Lucide icons by name"
      />
      <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
        {filteredCustomIcons.length} icon{filteredCustomIcons.length === 1 ? "" : "s"} — tap one to select.
      </p>
      <div className="poi-icon-picker-grid" role="listbox" aria-label="Lucide icons">
        {filteredCustomIcons.map(({ name, Icon }) => {
          const selected = selectedLucideIconName === name;
          return (
            <button
              key={name}
              type="button"
              role="option"
              aria-selected={selected}
              className={"poi-icon-cell" + (selected ? " poi-icon-cell-selected" : "")}
              title={name}
              onClick={() => handleIconPick(name)}
            >
              <Icon className="poi-icon-cell-glyph" aria-hidden />
              <span className="poi-icon-cell-name">{name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );

  return (
    <div className="modal-backdrop poi-details-modal-backdrop" onClick={onClose}>
      <div
        className="library-modal library-modal-tabs glass-panel poi-details-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="poi-details-modal-title"
      >
        <div className="library-header">
          <h2 id="poi-details-modal-title">Place tactical pin</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close pin details">
            ×
          </button>
        </div>

        <div className="library-tabs-scroll">
          <p className="library-desc poi-details-coords text-muted small">
            Coordinates: {latLng.lat.toFixed(5)}, {latLng.lng.toFixed(5)}
          </p>

          <label className="poi-details-name-label" htmlFor="poi-details-name-input">
            Pin name
          </label>
          <input
            id="poi-details-name-input"
            type="text"
            className="poi-details-name-input"
            value={pinName}
            onChange={(e) => setPinName(e.target.value)}
            placeholder="e.g. North cache, Relay hill"
            autoComplete="off"
            maxLength={200}
          />

          <label className="poi-details-name-label" htmlFor="poi-details-desc-input">
            Description
          </label>
          <textarea
            id="poi-details-desc-input"
            className="poi-details-description-input"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Short note about this pin (optional)"
            rows={3}
            maxLength={500}
          />

          <div className="poi-icon-summary-row">
            <span className="poi-details-name-label" style={{ marginBottom: 0 }}>
              Selected icon
            </span>
            <div className="poi-icon-preview-row">
              <div className="poi-icon-preview-tile poi-icon-preview-tile--static" aria-hidden>
                {DisplayIcon ? (
                  <DisplayIcon className="poi-icon-preview-glyph" aria-hidden />
                ) : (
                  <MapPin className="poi-icon-preview-glyph" aria-hidden />
                )}
                <span className="poi-icon-preview-name">{displayIconName}</span>
              </div>
              {selectedLucideIconName.trim().length > 0 ? (
                <button type="button" className="secondary poi-icon-clear" onClick={() => setSelectedLucideIconName("")}>
                  Use default pin
                </button>
              ) : null}
            </div>
          </div>

          <div className="library-tablist" role="tablist" aria-label="Pin icon category">
            <button
              type="button"
              role="tab"
              id="poi-tab-resources"
              aria-selected={activeTab === "resources"}
              aria-controls="poi-panel-icons"
              className={"library-tab" + (activeTab === "resources" ? " library-tab-active" : "")}
              onClick={() => setActiveTab("resources")}
            >
              Resources
            </button>
            <button
              type="button"
              role="tab"
              id="poi-tab-network"
              aria-selected={activeTab === "network"}
              aria-controls="poi-panel-icons"
              className={"library-tab" + (activeTab === "network" ? " library-tab-active" : "")}
              onClick={() => setActiveTab("network")}
            >
              Network
            </button>
            <button
              type="button"
              role="tab"
              id="poi-tab-custom"
              aria-selected={activeTab === "custom"}
              aria-controls="poi-panel-icons"
              className={"library-tab" + (activeTab === "custom" ? " library-tab-active" : "")}
              onClick={() => setActiveTab("custom")}
            >
              Custom
            </button>
          </div>

          <div className="library-tab-panels">
            <div className="library-tab-panel" id="poi-panel-icons" role="tabpanel" aria-labelledby={`poi-tab-${activeTab}`}>
              {activeTab === "resources" ? (
                <>
                  <p className="text-muted small poi-tab-hint">
                    Common supplies and sites. Pins save as <strong>resource</strong> markers on the map.
                  </p>
                  {resourcesGrid}
                </>
              ) : null}
              {activeTab === "network" ? (
                <>
                  <p className="text-muted small poi-tab-hint">
                    Radios, mesh, and connectivity. Pins save as <strong>network</strong> markers (shown with network layers).
                  </p>
                  {networkGrid}
                  <label className="poi-details-name-label" htmlFor="poi-network-coverage-m">
                    Coverage radius (meters)
                  </label>
                  <input
                    id="poi-network-coverage-m"
                    type="number"
                    inputMode="numeric"
                    min={0}
                    step={10}
                    className="poi-details-name-input"
                    value={networkCoverageRadiusM}
                    onChange={(e) => setNetworkCoverageRadiusM(e.target.value)}
                    placeholder="Optional — e.g. 500"
                    aria-describedby="poi-network-coverage-hint"
                  />
                  <p id="poi-network-coverage-hint" className="text-muted small" style={{ margin: "0.25rem 0 0" }}>
                    When set, a coverage circle appears on the map with mesh coverage (Network family → Coverage circles).
                  </p>
                </>
              ) : null}
              {activeTab === "custom" ? (
                <>
                  <p className="text-muted small poi-tab-hint">
                    Full icon library. These pins are saved as <strong>resource</strong> markers.
                  </p>
                  {customPanel}
                </>
              ) : null}
            </div>
          </div>

          {saveError !== null && <p className="poi-details-save-error library-tab-error">{saveError}</p>}

          <div className="poi-details-footer row">
            <button type="button" className="secondary" onClick={onClose} disabled={saving}>
              Cancel
            </button>
            <button type="button" className="poi-details-save-btn" onClick={() => void handleSave()} disabled={!canSave}>
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
