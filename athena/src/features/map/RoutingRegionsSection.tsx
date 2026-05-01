import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

type RoutingRegionRow = {
  id: string;
  name: string;
  bboxJson?: string;
  bbox_json?: string;
  sourcePbfPath?: string | null;
  source_pbf_path?: string | null;
  tilesPath?: string;
  tiles_path?: string;
  buildStatus?: string;
  build_status?: string;
  modesBuilt?: string;
  modes_built?: string;
  sizeBytes?: number;
  size_bytes?: number;
  createdAt?: string;
  created_at?: string;
  geocodeBuiltAt?: string | null;
  geocode_built_at?: string | null;
};

const GEOFABRIK_STARTERS: { name: string; href: string; hint: string }[] = [
  {
    name: "US Northeast (VT, NH, MA, …)",
    href: "https://download.geofabrik.de/north-america/us-northeast-latest.osm.pbf",
    hint: "~0.6–0.8 GB download; expect several GB of Valhalla tiles + ~100–200 MB geocode index.",
  },
  {
    name: "US Pacific Northwest",
    href: "https://download.geofabrik.de/north-america/us-pacific-latest.osm.pbf",
    hint: "Large; same tile/index caveats as other regions.",
  },
  {
    name: "Vermont (small extract)",
    href: "https://download.geofabrik.de/north-america/us/vermont-latest.osm.pbf",
    hint: "Smaller extract for testing.",
  },
];

function rowTilesPath(r: RoutingRegionRow): string {
  return (r.tilesPath ?? r.tiles_path ?? "").trim();
}

function rowBuildStatus(r: RoutingRegionRow): string {
  return (r.buildStatus ?? r.build_status ?? "").trim() || "ready";
}

function rowGeocodeAt(r: RoutingRegionRow): string | null {
  return r.geocodeBuiltAt ?? r.geocode_built_at ?? null;
}

function rowSizeBytes(r: RoutingRegionRow): number {
  const n = r.sizeBytes ?? r.size_bytes;
  return typeof n === "number" && Number.isFinite(n) ? n : 0;
}

function formatBytes(n: number): string {
  if (n <= 0) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/**
 * Valhalla routing regions: register tiles, build offline geocode from PBF, optional Docker sidecar.
 */
export function RoutingRegionsSection({ open: sectionOpen }: { open: boolean }) {
  const [regions, setRegions] = useState<RoutingRegionRow[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [valhallaUp, setValhallaUp] = useState(false);
  const [statusMsg, setStatusMsg] = useState("");
  const [regName, setRegName] = useState("Local region");
  const [tilesPath, setTilesPath] = useState("");
  const [bboxJson, setBboxJson] = useState('{"west":-100,"south":35,"east":-95,"north":40}');
  const [buildStatus, setBuildStatus] = useState("ready");
  const [busy, setBusy] = useState<string | null>(null);
  const [pbfByRegion, setPbfByRegion] = useState<Record<string, string>>({});
  const [autoRadiusKm, setAutoRadiusKm] = useState(60);
  const [autoMaxRegions, setAutoMaxRegions] = useState(12);
  const [autoLatLon, setAutoLatLon] = useState<{ lat: number; lon: number } | null>(null);

  const selectedRegion = useMemo(
    () => (activeId ? regions.find((r) => r.id === activeId) : null) ?? null,
    [activeId, regions],
  );

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<RoutingRegionRow[]>("valhalla_list_regions");
      setRegions(Array.isArray(list) ? list : []);
      const st = await invoke<{
        reachable: boolean;
        message: string;
        activeRoutingRegionId?: string | null;
        active_routing_region_id?: string | null;
      }>("valhalla_get_status");
      setValhallaUp(st.reachable);
      setActiveId((st.activeRoutingRegionId ?? st.active_routing_region_id ?? null) as string | null);
      setStatusMsg(st.reachable ? "Valhalla HTTP: reachable" : `Valhalla HTTP: ${st.message}`);
    } catch (e) {
      setStatusMsg(String(e));
    }
  }, []);

  useEffect(() => {
    if (!sectionOpen) return;
    void refresh();
  }, [sectionOpen, refresh]);

  useEffect(() => {
    if (!sectionOpen) return;
    void invoke<{ lastKnownGpsLat?: number | null; lastKnownGpsLng?: number | null }>("get_map_display_settings")
      .then((s) => {
        const lat = s?.lastKnownGpsLat ?? null;
        const lon = s?.lastKnownGpsLng ?? null;
        if (typeof lat === "number" && typeof lon === "number") {
          setAutoLatLon({ lat, lon });
        }
      })
      .catch(() => {});
  }, [sectionOpen]);

  const useMyLocationForAuto = useCallback(async () => {
    setBusy("Getting location…");
    setStatusMsg("");
    try {
      const s = await invoke<{ lastKnownGpsLat?: number | null; lastKnownGpsLng?: number | null }>("get_map_display_settings");
      let lat = s?.lastKnownGpsLat ?? null;
      let lon = s?.lastKnownGpsLng ?? null;
      if (lat == null || lon == null) {
        if (typeof navigator === "undefined" || !navigator.geolocation) {
          throw new Error("No saved GPS and browser location is unavailable.");
        }
        const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
          navigator.geolocation.getCurrentPosition(resolve, reject, {
            enableHighAccuracy: true,
            timeout: 12_000,
            maximumAge: 60_000,
          });
        });
        lat = pos.coords.latitude;
        lon = pos.coords.longitude;
      }
      setAutoLatLon({ lat: lat!, lon: lon! });
      setStatusMsg(`Using location: ${lat!.toFixed(5)}, ${lon!.toFixed(5)}`);
    } catch (e) {
      setStatusMsg(String(e));
    } finally {
      setBusy(null);
    }
  }, []);

  const autoDownloadNearby = useCallback(async () => {
    if (!autoLatLon) {
      setStatusMsg("Set a location first (Use my location).");
      return;
    }
    setBusy("Downloading regions…");
    setStatusMsg("");
    try {
      const created = await invoke<RoutingRegionRow[]>("geofabrik_download_regions_within_radius", {
        lat: autoLatLon.lat,
        lon: autoLatLon.lon,
        radius_km: autoRadiusKm,
        max_regions: autoMaxRegions,
        build_geocode: true,
      });
      setStatusMsg(`Downloaded & registered ${Array.isArray(created) ? created.length : 0} region(s).`);
      await refresh();
    } catch (e) {
      setStatusMsg(String(e));
    } finally {
      setBusy(null);
    }
  }, [autoLatLon, autoRadiusKm, autoMaxRegions, refresh]);

  const setPbfForActive = (path: string) => {
    if (!activeId) return;
    setPbfByRegion((m) => ({ ...m, [activeId]: path }));
  };

  const pickPbf = async () => {
    setBusy("Opening file…");
    try {
      const chosen = await open({
        multiple: false,
        title: "Open OSM .pbf for geocode build",
        filters: [{ name: "OSM PBF", extensions: ["pbf", "osm.pbf", "osm"] }],
      });
      const p = Array.isArray(chosen) ? chosen[0] : chosen;
      if (typeof p === "string" && p) setPbfForActive(p);
    } catch (e) {
      setStatusMsg(String(e));
    } finally {
      setBusy(null);
    }
  };

  const rebuildGeocode = async () => {
    if (!activeId) {
      setStatusMsg("Select an active region first.");
      return;
    }
    const pbf = pbfByRegion[activeId];
    if (!pbf?.trim()) {
      setStatusMsg("Choose a .pbf file (used to build the FTS place index for this region).");
      return;
    }
    setBusy("Building geocode index…");
    try {
      await invoke("geocode_rebuild_from_pbf_cmd", { region_id: activeId, pbf_path: pbf.trim() });
      setStatusMsg("Geocode index build completed.");
      await refresh();
    } catch (e) {
      setStatusMsg(String(e));
    } finally {
      setBusy(null);
    }
  };

  const dockerStart = async () => {
    if (!activeId) {
      setStatusMsg("Select an active region with a valid tiles_path first.");
      return;
    }
    setBusy("Starting Docker…");
    try {
      const msg = await invoke<string>("valhalla_docker_start", { region_id: activeId });
      setStatusMsg(msg);
      await refresh();
    } catch (e) {
      setStatusMsg(String(e));
    } finally {
      setBusy(null);
    }
  };

  const dockerStop = async () => {
    setBusy("Stopping Docker…");
    try {
      const msg = await invoke<string>("valhalla_docker_stop");
      setStatusMsg(msg);
      await refresh();
    } catch (e) {
      setStatusMsg(String(e));
    } finally {
      setBusy(null);
    }
  };

  const statusPillClass = valhallaUp ? "map-routing-pill--ok" : "map-routing-pill--err";

  return (
    <fieldset className="map-manager-toggles-fieldset">
      <legend className="map-manager-toggles-legend">Routing (Valhalla)</legend>
      <p className="text-muted small map-manager-toggles-hint">
        <strong>Offline geocode</strong> is built from a regional <code>.osm.pbf</code> (same area as your tiles).{" "}
        <strong>Valhalla</strong> serves road routes on <code>127.0.0.1:8002</code> when Docker is running. macOS: install
        Docker Desktop. Without Valhalla, the app still plots straight lines + distance.
      </p>
      <p className="text-muted small" style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
        <span>HTTP sidecar</span>
        <span className={"map-routing-pill " + statusPillClass} title="GET /status on the configured port">
          {valhallaUp ? "up" : "down"}
        </span>
        <span>{statusMsg}</span>
        {busy ? <span className="text-muted">({busy})</span> : null}
      </p>
      <label className="map-manager-control-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 4 }}>
        <span>Active region (geocode bias + tiles)</span>
        <select
          className="theme-select map-manager-select"
          value={activeId ?? ""}
          onChange={(e) => {
            const v = e.target.value.trim();
            void invoke("valhalla_set_active_region", { region_id: v.length > 0 ? v : null })
              .then(() => refresh())
              .catch((err) => setStatusMsg(String(err)));
          }}
        >
          <option value="">None</option>
          {regions.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name} — {rowBuildStatus(r)}
            </option>
          ))}
        </select>
      </label>

      {selectedRegion ? (
        <div
          className="map-routing-wizard glass-panel"
          style={{ marginTop: 10, padding: 8, display: "grid", gap: 6 }}
        >
          <div className="text-muted small" style={{ margin: 0 }}>
            <strong>{selectedRegion.name}</strong>
            <div>Tiles: {rowTilesPath(selectedRegion) || "—"}</div>
            <div>Tile dir size: {formatBytes(rowSizeBytes(selectedRegion))}</div>
            <div>Geocode built: {rowGeocodeAt(selectedRegion) ? new Date(rowGeocodeAt(selectedRegion)!).toLocaleString() : "—"}</div>
          </div>
          <div className="map-routing-wizard__step">
            <span className="text-muted small">1 — PBF for offline place search (geocoder)</span>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6, alignItems: "center" }}>
              <button type="button" className="secondary small-btn" onClick={() => void pickPbf()}>
                Choose .pbf
              </button>
              <span className="text-muted small" style={{ wordBreak: "break-all" }}>
                {pbfByRegion[activeId!] || "(not selected this session)"}
              </span>
            </div>
            <button type="button" onClick={() => void rebuildGeocode()} disabled={!pbfByRegion[activeId!]}>
              Build / rebuild geocode index
            </button>
          </div>
          <div className="map-routing-wizard__step">
            <span className="text-muted small">2 — Valhalla HTTP (requires built tiles on disk at tiles path)</span>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
              <button type="button" onClick={() => void dockerStart()}>
                Start (Docker)
              </button>
              <button type="button" className="secondary small-btn" onClick={() => void dockerStop()}>
                Stop
              </button>
            </div>
          </div>
        </div>
      ) : null}

      <div className="map-routing-wizard glass-panel" style={{ marginTop: 10, padding: 8, display: "grid", gap: 6 }}>
        <div className="text-muted small" style={{ margin: 0 }}>
          <strong>Auto-download nearby regions</strong>
          <div>
            Uses Geofabrik&apos;s region catalog (online once), downloads every region intersecting the radius, stores PBFs under app data,
            registers them, and builds offline geocode so Directions autocomplete works.
          </div>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, alignItems: "center" }}>
          <button type="button" className="secondary small-btn" onClick={() => void useMyLocationForAuto()}>
            Use my location
          </button>
          <span className="text-muted small">
            {autoLatLon ? `${autoLatLon.lat.toFixed(5)}, ${autoLatLon.lon.toFixed(5)}` : "(no location set)"}
          </span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
          <label>
            Radius (km)
            <input
              className="settings-input"
              inputMode="numeric"
              value={String(autoRadiusKm)}
              onChange={(e) => setAutoRadiusKm(Math.max(1, Math.min(2500, Number(e.target.value) || 0)))}
            />
          </label>
          <label>
            Max regions
            <input
              className="settings-input"
              inputMode="numeric"
              value={String(autoMaxRegions)}
              onChange={(e) => setAutoMaxRegions(Math.max(1, Math.min(200, Number(e.target.value) || 0)))}
            />
          </label>
        </div>
        <button type="button" onClick={() => void autoDownloadNearby()} disabled={!autoLatLon}>
          Download regions + build geocode
        </button>
        <p className="text-muted small" style={{ margin: 0 }}>
          Tip: Start small (e.g. 60 km, 5–12 regions). Larger radii can include huge downloads.
        </p>
      </div>

      <p className="text-muted small" style={{ marginTop: 8 }}>
        <strong>Starter extracts (Geofabrik)</strong> — download in a browser, then register tiles + point geocode PBF.
      </p>
      <ul className="text-muted small" style={{ listStyle: "none", padding: 0, margin: "0 0 8px" }}>
        {GEOFABRIK_STARTERS.map((s) => (
          <li key={s.href} style={{ margin: "4px 0" }}>
            <button
              type="button"
              className="linkish"
              onClick={() => {
                try {
                  window.open(s.href, "_blank", "noopener,noreferrer");
                } catch {
                  /* empty */
                }
              }}
            >
              {s.name}
            </button>
            <div>{s.hint}</div>
          </li>
        ))}
      </ul>
      <ul className="text-muted small" style={{ listStyle: "none", padding: 0, margin: "8px 0" }}>
        {regions.map((r) => (
          <li key={r.id} style={{ borderTop: "1px solid var(--border)", padding: "4px 0" }}>
            <strong>{r.name}</strong> — {rowTilesPath(r) || "no tiles path"} — {rowBuildStatus(r)}
            {rowGeocodeAt(r) ? ` — geocode ${rowGeocodeAt(r)?.slice(0, 10)}…` : ""}
          </li>
        ))}
        {regions.length === 0 ? <li>No regions registered yet.</li> : null}
      </ul>
      <div style={{ display: "grid", gap: 6, marginTop: 8 }}>
        <label>
          Name
          <input className="settings-input" value={regName} onChange={(e) => setRegName(e.target.value)} />
        </label>
        <label>
          Tiles path (absolute) — Valhalla tile build output directory
          <input
            className="settings-input"
            value={tilesPath}
            onChange={(e) => setTilesPath(e.target.value)}
            placeholder="/path/to/valhalla_tiles"
          />
        </label>
        <label>
          Bbox JSON
          <textarea className="settings-input" rows={2} value={bboxJson} onChange={(e) => setBboxJson(e.target.value)} />
        </label>
        <label>
          Build status (label)
          <input className="settings-input" value={buildStatus} onChange={(e) => setBuildStatus(e.target.value)} />
        </label>
        <button
          type="button"
          onClick={() => {
            void invoke<RoutingRegionRow>("valhalla_register_routing_region", {
              name: regName.trim(),
              tiles_path: tilesPath.trim(),
              bbox_json: (bboxJson ?? "").trim() || '{}',
              build_status: buildStatus.trim(),
            })
              .then(() => {
                void refresh();
              })
              .catch((e) => setStatusMsg(String(e)));
          }}
        >
          Register region
        </button>
      </div>
    </fieldset>
  );
}
