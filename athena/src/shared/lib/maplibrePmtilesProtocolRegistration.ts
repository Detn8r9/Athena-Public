import maplibregl from "maplibre-gl";
import { Protocol } from "pmtiles";

const globalRegistrationMarkerKey = "__athenaPmtilesMaplibreProtocolRegistered";

type GlobalRegistrationRecord = Record<string, boolean>;

function readGlobalRegistrationRecord(): GlobalRegistrationRecord {
  return globalThis as unknown as GlobalRegistrationRecord;
}

/**
 * Registers the `pmtiles` URL scheme with MapLibre GL JS exactly once for the lifetime of the JavaScript runtime.
 * Uses a `globalThis` marker so hot module replacement does not call `addProtocol` a second time after the module reloads.
 */
export function registerPmtilesMaplibreProtocolOnce(): void {
  const globalRecord = readGlobalRegistrationRecord();
  if (globalRecord[globalRegistrationMarkerKey] === true) {
    return;
  }

  const protocol = new Protocol({ metadata: true });
  maplibregl.addProtocol("pmtiles", protocol.tile);
  globalRecord[globalRegistrationMarkerKey] = true;
}
