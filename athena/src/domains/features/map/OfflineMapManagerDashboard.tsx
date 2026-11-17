import { useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";
import { motion } from "framer-motion";
import {
  MAP_CATALOG,
  bboxFromCenterAndRadiusMiles,
  tilesIntersectingCircle,
  MAX_EXTRACT_RADIUS_MILES,
} from "../../../shared/lib/MapCatalogIndex";
import { MapArchiveMapTypeCard } from "./MapArchiveMapTypeCard";
import { ErrorBoundary } from "../../../app/ErrorBoundary";
import { MapArchivePreviewMap } from "./MapArchivePreviewMap";

export interface OfflineMapManagerDashboardProps {
  open: boolean;
  onClose: () => void;
  savedUserLocationAddress: string;
  tileGeocodeOverride: string;
  tileRadiusMiles: number;
  onTileRadiusMilesChange: (miles: number) => void;
  /** When set, map tile acquisition is routed to the Data Acquisition hub instead of in-map downloads. */
  onOpenDataAcquisition?: () => void;
}

export function OfflineMapManagerDashboard({
  open,
  onClose,
  savedUserLocationAddress,
  tileGeocodeOverride,
  tileRadiusMiles,
  onTileRadiusMilesChange,
  onOpenDataAcquisition,
}: OfflineMapManagerDashboardProps) {
  const [locationDraft, setLocationDraft] = useState("");
  const [locationStatus, setLocationStatus] = useState<string | null>(null);
  const [locationExpanded, setLocationExpanded] = useState(true);
  const [center, setCenter] = useState<{ lat: number; lon: number } | null>(null);
  const [geocodeError, setGeocodeError] = useState<string | null>(null);
  const [geocoding, setGeocoding] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    const initial = (savedUserLocationAddress ?? "").trim();
    setLocationDraft(initial);
    setLocationExpanded(initial.length === 0);
    setLocationStatus(null);
  }, [open, savedUserLocationAddress]);

  const saveLocation = async () => {
    const v = locationDraft.trim();
    setLocationStatus(null);
    try {
      await invoke("set_user_location", { value: v.length > 0 ? v : null });
      setLocationStatus(v.length > 0 ? "Saved. This location drives downloads and the preview map." : "Cleared saved location.");
      if (v.length > 0) {
        setLocationExpanded(false);
      }
    } catch (e) {
      setLocationStatus(String(e));
    }
  };

  const geocodeQuery = (tileGeocodeOverride.trim() || locationDraft.trim() || savedUserLocationAddress.trim()).trim();
  const radiusMiles = Math.max(0.1, Math.min(tileRadiusMiles, MAX_EXTRACT_RADIUS_MILES));

  useEffect(() => {
    if (!open) {
      return;
    }
    if (geocodeQuery.length === 0) {
      setCenter(null);
      setGeocodeError("Enter a place name or set your location.");
      return;
    }
    let cancelled = false;
    setGeocoding(true);
    setGeocodeError(null);
    void invoke<[number, number]>("geocode_place_query", { query: geocodeQuery })
      .then((geo) => {
        if (cancelled) {
          return;
        }
        setCenter({ lat: geo[0], lon: geo[1] });
        setGeocoding(false);
        setGeocodeError(null);
      })
      .catch((e) => {
        if (cancelled) {
          return;
        }
        setCenter(null);
        setGeocoding(false);
        setGeocodeError(e != null ? String(e) : "Could not resolve a location.");
      });
    return () => {
      cancelled = true;
    };
  }, [open, geocodeQuery]);

  const indexTiles = useMemo(() => {
    if (!center) {
      return [];
    }
    return tilesIntersectingCircle(center.lat, center.lon, radiusMiles);
  }, [center, radiusMiles]);

  const bbox = useMemo(
    () => (center ? bboxFromCenterAndRadiusMiles(center.lat, center.lon, radiusMiles) : null),
    [center, radiusMiles],
  );

  if (!open) {
    return null;
  }

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <motion.div
        className="library-modal offline-map-manager-modal map-archive-manager glass-panel"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="offline-map-manager-title"
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: 12 }}
        transition={{ type: "spring", stiffness: 320, damping: 32 }}
      >
        <div className="map-archive-manager__head map-routing-setup-modal__head--sticky">
          <h2 id="offline-map-manager-title" className="map-archive-manager__h2">
            Download maps
          </h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <p className="map-archive-manager__lead text-muted small">
          Pick a place and how far around it, then download a map of that area so the Map screen works without
          internet.
        </p>

        <div
          className={`map-manager-location glass-panel${locationExpanded ? "" : " map-manager-location--compact"}`}
          aria-label="Location"
        >
          <div className="map-manager-location__head">
            <strong>Location</strong>
            {!locationExpanded ? (
              <span className="text-muted small">{locationDraft.trim() || "Not set"}</span>
            ) : (
              <span className="text-muted small">Name or address used to place the map and run downloads.</span>
            )}
          </div>
          {locationExpanded ? (
            <>
              <div className="map-manager-location__row">
                <input
                  className="settings-input"
                  value={locationDraft}
                  onChange={(e) => setLocationDraft(e.target.value)}
                  placeholder="City, region, or full address"
                />
                <button type="button" className="secondary small-btn" onClick={() => void saveLocation()}>
                  Save
                </button>
              </div>
              {locationStatus ? (
                <div
                  className={`small ${String(locationStatus).toLowerCase().includes("saved") || String(locationStatus).toLowerCase().includes("cleared") ? "text-muted" : "error"}`}
                >
                  {locationStatus}
                </div>
              ) : null}
              {tileGeocodeOverride.trim() ? (
                <div className="text-muted small">
                  Settings map override: <code>{tileGeocodeOverride.trim()}</code> (takes priority for geocoding)
                </div>
              ) : null}
            </>
          ) : (
            <div className="map-manager-location__row map-manager-location__row--compact">
              <button
                type="button"
                className="secondary small-btn"
                onClick={() => {
                  setLocationExpanded(true);
                  setLocationStatus(null);
                }}
              >
                Change
              </button>
            </div>
          )}
        </div>

        <div className="map-archive-radius">
          <label className="map-archive-radius__label" htmlFor="map-archive-radius-mi">
            <span>How far around it: </span>
            <strong className="map-archive-radius__value">{tileRadiusMiles}</strong>
            <span className="text-muted"> mi</span>{" "}
            <span className="text-muted small" style={{ fontWeight: 400 }}>(up to {MAX_EXTRACT_RADIUS_MILES} mi)</span>
          </label>
          <input
            id="map-archive-radius-mi"
            className="memory-slider"
            type="range"
            min={1}
            max={MAX_EXTRACT_RADIUS_MILES}
            step={1}
            value={Math.min(Math.max(1, tileRadiusMiles), MAX_EXTRACT_RADIUS_MILES)}
            onChange={(e) => onTileRadiusMilesChange(Number(e.target.value) || 1)}
            aria-label="Download radius in miles"
          />
        </div>

        {geocoding && (
          <p className="text-muted small map-archive-manager__subtle" style={{ textAlign: "center" }}>
            Finding the place…
          </p>
        )}
        {geocodeError && !geocoding ? (
          <p className="quartermaster-hub-err small" style={{ textAlign: "center" }} role="alert">
            {geocodeError}
          </p>
        ) : null}

        <div className="map-archive-manager__preview" role="region" aria-label="Download preview map">
          <ErrorBoundary sectionLabel="map download preview (MapLibre)">
            <MapArchivePreviewMap center={center} radiusMiles={radiusMiles} tiles={indexTiles} />
          </ErrorBoundary>
        </div>

        <div className="map-archive-manager__recs" aria-label="Map downloads">
          <h3 className="map-archive-manager__h3">Download this area</h3>
          {MAP_CATALOG.filter((m) => m.acquirable).map((m) => (
            <MapArchiveMapTypeCard
              key={m.id}
              mapType={m}
              radiusMiles={radiusMiles}
              canRun={bbox != null && !geocoding}
              minLon={bbox?.min_lon ?? 0}
              minLat={bbox?.min_lat ?? 0}
              maxLon={bbox?.max_lon ?? 0}
              maxLat={bbox?.max_lat ?? 0}
              locationLabel={geocodeQuery}
              open={open}
            />
          ))}
          {onOpenDataAcquisition ? (
            <p className="text-muted small" style={{ margin: "0.75rem 0 0" }}>
              Need a whole country or the world?{" "}
              <button
                type="button"
                className="store-inline-link"
                onClick={() => {
                  onClose();
                  onOpenDataAcquisition();
                }}
              >
                Browse ready-made maps
              </button>
            </p>
          ) : null}
        </div>
      </motion.div>
    </div>
  );
}
