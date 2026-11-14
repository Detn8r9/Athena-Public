import { describe, expect, it } from "vitest";
import type { MapGeoJSONFeature } from "maplibre-gl";
import { buildingLayerIds, placeFromFeatures, streetMapFiles, streetMapLayerSpecs } from "./streetMapLayers";
import { metresBetween } from "./PlaceCard";

const feature = (sourceLayer: string, properties: Record<string, unknown>) =>
  ({ sourceLayer, properties }) as unknown as MapGeoJSONFeature;

describe("offline street maps", () => {
  it("uses only street map extracts, newest first", () => {
    expect(streetMapFiles(["satellite_home", "protomaps_region_1700", "protomaps_region_1800", "noaa_x"])).toEqual([
      "protomaps_region_1800.pmtiles",
      "protomaps_region_1700.pmtiles",
    ]);
  });

  it("draws roads, buildings and labels but never a world background", () => {
    const ids = streetMapLayerSpecs("s0", false).map((l) => l.id);
    expect(ids).toContain("s0:buildings");
    expect(ids).toContain("s0:roads_labels_minor");
    expect(ids).toContain("s0:address_label");
    expect(ids.some((id) => id.endsWith(":background"))).toBe(false);
    expect(buildingLayerIds(ids)).toEqual(["s0:buildings"]);
  });

  it("keeps imagery visible: only roads and labels over aerial photos", () => {
    const ids = streetMapLayerSpecs("s0", true).map((l) => l.id);
    expect(ids).toContain("s0:roads_highway");
    expect(ids).toContain("s0:places_locality");
    expect(ids.some((id) => /:(earth|water|landuse_park|buildings)$/.test(id))).toBe(false);
  });

  it("reads the address a building carries", () => {
    const p = placeFromFeatures([feature("roads", { name: "Main St" }), feature("buildings", { addr_housenumber: "412", addr_street: "Main St", kind: "building" })], 44, -103);
    expect(p).toEqual({ lat: 44, lng: -103, address: "412 Main St", name: null, kind: null });
    expect(placeFromFeatures([feature("pois", { name: "Fire Station 3", kind: "fire_station" })], 1, 2)?.name).toBe("Fire Station 3");
    expect(placeFromFeatures([feature("roads", { name: "Main St" })], 1, 2)).toBeNull();
  });

  it("measures distance for the nearest-address check", () => {
    expect(Math.round(metresBetween(44, -103, 44.0005, -103))).toBe(56);
  });
});
