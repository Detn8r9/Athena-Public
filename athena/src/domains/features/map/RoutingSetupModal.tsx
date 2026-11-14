import { useEffect, useMemo, useState } from "react";
import { athenaInvoke as invoke, athenaListen as listen } from "../../../shared/ipc/athenaIpc";
import { createPortal } from "react-dom";
import { motion, AnimatePresence } from "framer-motion";
import { X, AlertTriangle } from "lucide-react";
import { MapArchivePreviewMap } from "./MapArchivePreviewMap";
import { tilesIntersectingCircle } from "../../../shared/lib/MapCatalogIndex";

type ValhallaStatus = { reachable: boolean; message: string };
type RoadRouterStatus = { ready: boolean; roads: number; sizeBytes: number };
type RoutingRegionRow = { id: string; name: string; tiles_path?: string; tilesPath?: string };

function milesToKm(mi: number): number {
  return mi * 1.60934;
}

export function RoutingSetupModal({
  open,
  onClose,
  savedUserLocationAddress,
  onOpenDataAcquisition,
}: {
  open: boolean;
  onClose: () => void;
  savedUserLocationAddress: string;
  onOpenDataAcquisition?: () => void;
}) {
  const [locationDraft, setLocationDraft] = useState("");
  const [locationExpanded, setLocationExpanded] = useState(true);
  const [radiusMiles, setRadiusMiles] = useState(60);
  const [center, setCenter] = useState<{ lat: number; lon: number } | null>(null);
  const [geocodeError, setGeocodeError] = useState<string | null>(null);
  const [geocoding, setGeocoding] = useState(false);
  const [valhalla, setValhalla] = useState<ValhallaStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const [engineStarting, setEngineStarting] = useState(false);
  const [engineStartHint, setEngineStartHint] = useState<string | null>(null);
  const [regions, setRegions] = useState<RoutingRegionRow[]>([]);
  const [activeRegionId, setActiveRegionId] = useState<string | null>(null);
  const [cycling, setCycling] = useState(false);
  const [roadRouter, setRoadRouter] = useState<RoadRouterStatus | null>(null);
  const [roadBuilding, setRoadBuilding] = useState(false);
  const [roadMsg, setRoadMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    void invoke<RoadRouterStatus>("road_router_status")
      .then((s) => setRoadRouter(s))
      .catch(() => setRoadRouter(null));
    let unlisten: (() => void) | undefined;
    void listen<{ message?: string }>("road-router-progress", (e) => {
      if (e.payload?.message) setRoadMsg(e.payload.message);
    }).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
  }, [open]);

  const [areaBusy, setAreaBusy] = useState(false);
  const [areaMsg, setAreaMsg] = useState<string | null>(null);
  const downloadMyArea = async () => {
    setAreaBusy(true);
    setAreaMsg(null);
    try {
      const r = await invoke<{ imageryRadiusMiles: number; streetMapJob: string | null; imageryJob: string | null; problems: string[] }>(
        "maps_download_my_area",
      );
      const parts = [
        r.streetMapJob ? "street map (100 mi)" : null,
        r.imageryJob ? `aerial imagery (${Math.round(r.imageryRadiusMiles)} mi)` : null,
        "road and address data",
      ].filter(Boolean);
      setAreaMsg(`Downloading ${parts.join(", ")}. This runs in the background and can take an hour or more.${r.problems.length ? " " + r.problems.join(" ") : ""}`);
    } catch (e) {
      setAreaMsg(String(e));
    } finally {
      setAreaBusy(false);
    }
  };

  const buildRoadData = async () => {
    setRoadBuilding(true);
    setRoadMsg("Building road data…");
    try {
      const s = await invoke<RoadRouterStatus>("road_router_build");
      setRoadRouter(s);
    } catch (e) {
      setRoadMsg(String(e));
    } finally {
      setRoadBuilding(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    const initial = (savedUserLocationAddress ?? "").trim();
    setLocationDraft(initial);
    setLocationExpanded(initial.length === 0);
    setStatusMsg(null);
    setCenter(null);
    setGeocodeError(null);
  }, [open, savedUserLocationAddress]);

  const geocodeQuery = locationDraft.trim();

  useEffect(() => {
    if (!open) return;
    void invoke<{ reachable: boolean; message: string }>("valhalla_get_status")
      .then((s) => setValhalla({ reachable: Boolean(s?.reachable), message: String(s?.message ?? "") }))
      .catch((e) => setValhalla({ reachable: false, message: String(e) }));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    void Promise.all([invoke<RoutingRegionRow[]>("valhalla_list_regions"), invoke<{ activeRoutingRegionId?: string | null; active_routing_region_id?: string | null }>("valhalla_get_status")])
      .then(([list, st]) => {
        setRegions(Array.isArray(list) ? list : []);
        setActiveRegionId((st.activeRoutingRegionId ?? st.active_routing_region_id ?? null) as string | null);
      })
      .catch(() => {
        setRegions([]);
        setActiveRegionId(null);
      });
  }, [open]);

  const activeRegionPath = useMemo(() => {
    if (!activeRegionId) return null;
    const r = regions.find((x) => x.id === activeRegionId);
    const tiles = (r?.tilesPath ?? r?.tiles_path ?? "").trim();
    if (!tiles) return null;
    // Backend expects VALHALLA_CUSTOM_FILES (parent of valhalla_tiles).
    const parts = tiles.split(/[\\/]/g).filter(Boolean);
    parts.pop();
    return (tiles.startsWith("/") ? "/" : "") + parts.join("/");
  }, [regions, activeRegionId]);

  const refreshValhalla = async () => {
    try {
      const s = await invoke<{ reachable: boolean; message: string }>("valhalla_get_status");
      setValhalla({ reachable: Boolean(s?.reachable), message: String(s?.message ?? "") });
      return Boolean(s?.reachable);
    } catch (e) {
      setValhalla({ reachable: false, message: String(e) });
      return false;
    }
  };

  useEffect(() => {
    if (!open) return;
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
        if (cancelled) return;
        setCenter({ lat: geo[0], lon: geo[1] });
        setGeocoding(false);
      })
      .catch(() => {
        if (cancelled) return;
        setCenter(null);
        setGeocoding(false);
        setGeocodeError("Could not resolve that location.");
      });
    return () => {
      cancelled = true;
    };
  }, [open, geocodeQuery]);

  const tiles = useMemo(() => {
    if (!center) return [];
    return tilesIntersectingCircle(center.lat, center.lon, Math.max(1, radiusMiles));
  }, [center, radiusMiles]);

  const estimatedRegions = useMemo(() => {
    // Heuristic: more tiles → more geofabrik regions. Keep it stable & simple.
    const t = tiles.length;
    if (t <= 6) return 1;
    if (t <= 14) return 2;
    if (t <= 26) return 4;
    return Math.min(12, Math.max(6, Math.round(t / 5)));
  }, [tiles.length]);

  // Downloading routing/geocode data should not require Valhalla to be running yet.
  // Valhalla is only required to actually compute routed paths.
  const canDownload = Boolean(center) && !geocoding && !geocodeError && !busy;

  const startEngine = async () => {
    setEngineStartHint(null);
    setEngineStarting(true);
    try {
      await invoke<string>("start_valhalla_engine", { activeRegionPath });
      // Poll health until reachable (or timeout).
      const startedAt = Date.now();
      while (Date.now() - startedAt < 12_000) {
        const ok = await refreshValhalla();
        if (ok) {
          setEngineStarting(false);
          return;
        }
        // eslint-disable-next-line no-await-in-loop
        await new Promise((r) => window.setTimeout(r, 900));
      }
      setEngineStartHint("Valhalla is still starting. If it doesn’t come up, check Docker Desktop.");
    } catch (e) {
      const msg = String(e ?? "");
      if (msg.toLowerCase().includes("docker daemon is not running")) {
        setEngineStartHint("Docker daemon is offline. System-level launch required.");
      } else {
        setEngineStartHint("Could not start Valhalla engine.");
      }
    } finally {
      setEngineStarting(false);
      void refreshValhalla();
    }
  };

  const cycleEngineForRegion = async (nextRegionId: string) => {
    setCycling(true);
    setStatusMsg("Cycling routing engine…");
    setEngineStartHint(null);
    try {
      await invoke("valhalla_set_active_region", { region_id: nextRegionId.length ? nextRegionId : null });
      setActiveRegionId(nextRegionId.length ? nextRegionId : null);

      await invoke("stop_valhalla_engine");
      await invoke("start_valhalla_engine", { activeRegionPath: (() => {
        const r = regions.find((x) => x.id === nextRegionId);
        const tiles = (r?.tilesPath ?? r?.tiles_path ?? "").trim();
        if (!tiles) return null;
        const parts = tiles.split(/[\\/]/g).filter(Boolean);
        parts.pop();
        return (tiles.startsWith("/") ? "/" : "") + parts.join("/");
      })() });

      const startedAt = Date.now();
      while (Date.now() - startedAt < 14_000) {
        const ok = await refreshValhalla();
        if (ok) {
          setStatusMsg(null);
          setCycling(false);
          return;
        }
        // eslint-disable-next-line no-await-in-loop
        await new Promise((r) => window.setTimeout(r, 900));
      }
      setStatusMsg("Engine is still starting…");
    } catch (e) {
      const msg = String(e ?? "");
      if (msg.toLowerCase().includes("docker daemon is not running")) {
        setEngineStartHint("Docker daemon is offline. System-level launch required.");
      } else {
        setStatusMsg("Could not cycle routing engine.");
      }
    } finally {
      setCycling(false);
      void refreshValhalla();
    }
  };

  const downloadRoutingData = async () => {
    if (!center) return;
    setBusy(true);
    setStatusMsg(null);
    try {
      await invoke("geofabrik_download_regions_within_radius", {
        lat: center.lat,
        lon: center.lon,
        radius_km: milesToKm(radiusMiles),
        max_regions: estimatedRegions,
        build_geocode: true,
      });
      setStatusMsg("Routing data downloaded. You can now enable a region and start routing.");
    } catch (e) {
      const msg = String(e ?? "").trim();
      setStatusMsg(msg ? `Download failed: ${msg}` : "Download failed. Check network connectivity and try again.");
    } finally {
      setBusy(false);
    }
  };

  if (!open) return null;
  const modal = (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <motion.div
        className="library-modal map-routing-setup-modal map-routing-setup-modal--new glass-panel"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Routing setup"
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: 12 }}
        transition={{ type: "spring", stiffness: 320, damping: 32 }}
      >
        <div className="map-routing-setup-modal__head map-routing-setup-modal__head--sticky">
          <h2 className="map-routing-setup-modal__h2">Routing setup</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close routing setup">
            <X size={16} />
          </button>
        </div>

        <div
          className={`map-manager-location glass-panel${locationExpanded ? "" : " map-manager-location--compact"}`}
          aria-label="Location"
          style={{ marginLeft: 0, marginRight: 0 }}
        >
          <div className="map-manager-location__head">
            <strong>Location</strong>
            {!locationExpanded ? (
              <span className="text-muted small">{locationDraft.trim() || "Not set"}</span>
            ) : (
              <span className="text-muted small">Used to pick nearby routing regions.</span>
            )}
          </div>
          {locationExpanded ? (
            <div className="map-manager-location__row">
              <input
                className="settings-input"
                value={locationDraft}
                onChange={(e) => setLocationDraft(e.target.value)}
                placeholder="City, region, or full address"
              />
              <button type="button" className="secondary small-btn" onClick={() => setLocationExpanded(false)} disabled={!locationDraft.trim()}>
                Set
              </button>
            </div>
          ) : (
            <div className="map-manager-location__row map-manager-location__row--compact">
              <button type="button" className="secondary small-btn" onClick={() => setLocationExpanded(true)}>
                Change
              </button>
            </div>
          )}
        </div>

        <div className="map-archive-radius" style={{ marginLeft: 0, marginRight: 0 }}>
          <label className="map-archive-radius__label" htmlFor="routing-radius-mi">
            <span>Radius: </span>
            <strong className="map-archive-radius__value">{radiusMiles}</strong>
            <span className="text-muted"> mi</span>
          </label>
          <input
            id="routing-radius-mi"
            className="memory-slider"
            type="range"
            min={5}
            max={250}
            step={5}
            value={radiusMiles}
            onChange={(e) => setRadiusMiles(Number(e.target.value) || 60)}
          />
        </div>

        {geocoding ? (
          <p className="text-muted small" style={{ margin: "0 0 10px", textAlign: "center" }}>
            Resolving place…
          </p>
        ) : geocodeError ? (
          <p className="quartermaster-hub-err small" style={{ margin: "0 0 10px", textAlign: "center" }} role="alert">
            {geocodeError}
          </p>
        ) : null}

        <div className="map-archive-manager__preview" style={{ marginLeft: 0, marginRight: 0 }}>
          <MapArchivePreviewMap center={center} radiusMiles={radiusMiles} tiles={tiles} />
        </div>

        <motion.article className="map-archive-card" style={{ marginBottom: 0 }}>
          <h3 className="map-archive-card__title">Offline directions engine</h3>
          <div style={{ display: "grid", gap: 8 }}>
            <label className="small" style={{ display: "grid", gap: 4 }}>
              <span className="text-muted">Active region</span>
              <select
                className="theme-select"
                value={activeRegionId ?? ""}
                onChange={(e) => {
                  const v = String(e.target.value ?? "");
                  void cycleEngineForRegion(v);
                }}
                disabled={cycling || engineStarting}
              >
                <option value="">None</option>
                {regions.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.name}
                  </option>
                ))}
              </select>
            </label>
            <p className="map-archive-card__body text-muted" style={{ margin: 0 }}>
              Estimated regions for this radius: <strong>{estimatedRegions}</strong>
            </p>
          </div>

          <div className="map-archive-card__body" style={{ display: "grid", gap: 6 }}>
            <strong>Maps for your area</strong>
            <span className="text-muted small">
              One step for everything: street map with road names and buildings (100 miles), aerial imagery close in,
              and road and address data for directions. Uses the station&apos;s saved location.
            </span>
            <button type="button" onClick={() => void downloadMyArea()} disabled={areaBusy} style={{ justifySelf: "start", minHeight: 44 }}>
              {areaBusy ? "Starting…" : "Download maps for my area"}
            </button>
            {areaMsg ? <span className="small">{areaMsg}</span> : null}
            <strong style={{ marginTop: 8 }}>Built-in directions</strong>
            <span className="text-muted small">
              {roadRouter?.ready
                ? `Ready: ${roadRouter.roads.toLocaleString()} road sections. Works with no internet and no extra programs.`
                : "Uses the road data downloaded for your area. Build it once after downloading regions."}
            </span>
            {roadMsg ? <span className="small">{roadMsg}</span> : null}
            <button type="button" className="secondary" onClick={() => void buildRoadData()} disabled={roadBuilding} style={{ justifySelf: "start", minHeight: 40 }}>
              {roadBuilding ? "Building…" : roadRouter?.ready ? "Rebuild road data" : "Build road data"}
            </button>
          </div>

          <AnimatePresence>
            {!valhalla?.reachable && !roadRouter?.ready ? (
              <motion.div
                className="map-archive-card__warning"
                initial={{ opacity: 0, y: -6, scale: 0.98 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: -4, scale: 0.99 }}
                transition={{ type: "spring", stiffness: 360, damping: 28 }}
                role="alert"
              >
                <AlertTriangle size={14} aria-hidden />
                <span>{engineStartHint ?? "Requires Valhalla engine to be running."}</span>
                <button
                  type="button"
                  className="linkish"
                  style={{ marginLeft: "auto" }}
                  onClick={() => void startEngine()}
                  disabled={engineStarting}
                >
                  {engineStarting ? "Starting engine…" : "Start Valhalla engine"}
                </button>
              </motion.div>
            ) : null}
          </AnimatePresence>

          <div className="map-archive-card__action">
            {onOpenDataAcquisition ? (
              <button
                type="button"
                className="map-archive-card__dl"
                onClick={() => {
                  onClose();
                  onOpenDataAcquisition();
                }}
              >
                Open Downloads (Maps)
              </button>
            ) : (
              <button type="button" className="map-archive-card__dl" onClick={() => void downloadRoutingData()} disabled={!canDownload}>
                {busy ? "Downloading…" : "Download routing data"}
              </button>
            )}
          </div>

          {statusMsg ? (
            <div
              className={`map-routing-setup-modal__status ${
                statusMsg.toLowerCase().includes("failed") ? "map-routing-setup-modal__status--err" : "map-routing-setup-modal__status--ok"
              }`}
              role={statusMsg.toLowerCase().includes("failed") ? "alert" : undefined}
            >
              {statusMsg}
            </div>
          ) : null}
        </motion.article>
      </motion.div>
    </div>
  );

  return typeof document !== "undefined" ? createPortal(modal, document.body) : modal;
}

