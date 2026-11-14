/**
 * Offline street map: draws downloaded Protomaps vector extracts (`protomaps_region_*.pmtiles`)
 * on the main map with road names, buildings, house numbers, places and points of interest.
 * Fonts and icons are bundled under `public/map-assets`, so labels draw with no internet.
 */
import { layers as protomapsLayers, namedFlavor } from "@protomaps/basemaps";
import type { LayerSpecification, Map as MlMap, MapGeoJSONFeature } from "maplibre-gl";
import { buildPmtilesMapLibreUrl } from "./offlineMapArchiveUrls";

/** File-name prefix of street map extracts made by `extract_pmtiles_region`. */
export const STREET_MAP_PREFIX = "protomaps_region_";
const SOURCE_PREFIX = "athena-street-";

/** Absolute URLs for the bundled glyphs and sprite (MapLibre needs absolute URLs). */
export function streetMapAssetUrls(): { glyphs: string; sprite: string } {
  const base = typeof window !== "undefined" ? new URL("/map-assets/", window.location.href).href : "/map-assets/";
  return { glyphs: `${base}fonts/{fontstack}/{range}.pbf`, sprite: `${base}sprites/light` };
}

/** Downloaded street map archive file names, newest first. */
export function streetMapFiles(downloadedIds: string[]): string[] {
  return downloadedIds
    .filter((id) => id.startsWith(STREET_MAP_PREFIX))
    .sort()
    .reverse()
    .map((id) => `${id}.pmtiles`);
}

/** Layers that stay readable on top of aerial imagery. */
const OVER_IMAGERY = /^(roads_|boundaries|places_|pois$|address_label$|water_waterway_label$|water_label|earth_label)/;

/**
 * Street map layers for one archive. `overImagery` keeps only roads and labels so aerial imagery
 * shows through. The world `background` layer is always dropped so areas outside the download
 * still show the base map.
 */
export function streetMapLayerSpecs(sourceId: string, overImagery: boolean): LayerSpecification[] {
  return (protomapsLayers(sourceId, namedFlavor("light"), { lang: "en" }) as LayerSpecification[])
    .filter((l) => l.type !== "background")
    .filter((l) => !overImagery || OVER_IMAGERY.test(l.id))
    .map((l) => ({ ...l, id: `${sourceId}:${l.id}` }) as LayerSpecification);
}

/** Adds every downloaded street map below `beforeLayerId` (ATHENA's own markers stay on top). */
export function addStreetMaps(map: MlMap, files: string[], overImagery: boolean, beforeLayerId?: string): string[] {
  const added: string[] = [];
  // Oldest first, so the newest download draws on top where extracts overlap.
  [...files].reverse().forEach((file, i) => {
    const sourceId = `${SOURCE_PREFIX}${i}`;
    const specs = streetMapLayerSpecs(sourceId, overImagery);
    if (map.getSource(sourceId)) {
      // Already drawn (the effect ran again): report its layers so taps still work.
      added.push(...specs.map((l) => l.id).filter((id) => map.getLayer(id)));
      return;
    }
    map.addSource(sourceId, {
      type: "vector",
      url: buildPmtilesMapLibreUrl(file),
      attribution: "© OpenStreetMap contributors, Protomaps",
    });
    for (const layer of specs) {
      map.addLayer(layer, beforeLayerId && map.getLayer(beforeLayerId) ? beforeLayerId : undefined);
      added.push(layer.id);
    }
  });
  return added;
}

/** Building layer ids among the street map layers that were added. */
export function buildingLayerIds(layerIds: string[]): string[] {
  return layerIds.filter((id) => id.endsWith(":buildings"));
}

export type PlaceAtPoint = {
  lat: number;
  lng: number;
  /** House number and street from the map data, when the building carries them. */
  address: string | null;
  name: string | null;
  kind: string | null;
};

/** What the map data says about a tapped building or point of interest. */
export function placeFromFeatures(features: MapGeoJSONFeature[], lat: number, lng: number): PlaceAtPoint | null {
  const f = features.find((x) => {
    const p = x.properties ?? {};
    return x.sourceLayer === "buildings" || x.sourceLayer === "pois" || p.addr_housenumber != null;
  });
  if (!f) return null;
  const p = (f.properties ?? {}) as Record<string, unknown>;
  const str = (v: unknown) => (typeof v === "string" && v.trim() ? v.trim() : null);
  const num = str(p.addr_housenumber) ?? (typeof p.addr_housenumber === "number" ? String(p.addr_housenumber) : null);
  const street = str(p.addr_street);
  const address = num && street ? `${num} ${street}` : num ? `No. ${num}` : null;
  const name = str(p["name:en"]) ?? str(p.name);
  const kind = str(p.kind_detail) ?? str(p.kind);
  return { lat, lng, address, name, kind: kind === "building" ? null : kind };
}
