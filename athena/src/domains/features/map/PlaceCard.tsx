/**
 * Card for a tapped building or place: name, address and quick actions. The address comes from the
 * street map itself when the building carries one, otherwise from the offline address index.
 */
import { useEffect, useState } from "react";
import { athenaInvoke as invoke } from "../../../shared/ipc/athenaIpc";
import type { PlaceAtPoint } from "./streetMapLayers";
import "./PlaceCard.css";

type GeocodeMatch = { label: string; lat: number; lon: number; featureClass?: string };

/** Distance in metres between two points (good enough for "is this the same building"). */
export function metresBetween(aLat: number, aLng: number, bLat: number, bLng: number): number {
  const r = 6371000;
  const dLat = ((bLat - aLat) * Math.PI) / 180;
  const dLng = ((bLng - aLng) * Math.PI) / 180;
  const x = Math.sin(dLat / 2) ** 2 + Math.cos((aLat * Math.PI) / 180) * Math.cos((bLat * Math.PI) / 180) * Math.sin(dLng / 2) ** 2;
  return 2 * r * Math.asin(Math.sqrt(x));
}

/** The nearest indexed address counts only if it is close enough to be this building. */
export const NEAREST_ADDRESS_MAX_METRES = 60;

export function PlaceCard({
  place,
  onClose,
  onDirections,
  onAddMarker,
}: {
  place: PlaceAtPoint;
  onClose: () => void;
  onDirections: (p: { lat: number; lng: number }) => void;
  onAddMarker: (p: { lat: number; lng: number }) => void;
}) {
  const [nearby, setNearby] = useState<string | null>(null);
  const [looking, setLooking] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setNearby(null);
    // A house number without a street (common in map data) still needs the index for the street.
    if (place.address && !place.address.startsWith("No. ")) return;
    setLooking(true);
    void invoke<GeocodeMatch[]>("geocode_reverse_cmd", { lat: place.lat, lon: place.lng, max_results: 1 })
      .then((hits) => {
        const h = Array.isArray(hits) ? hits[0] : undefined;
        if (!cancelled && h && metresBetween(place.lat, place.lng, h.lat, h.lon) <= NEAREST_ADDRESS_MAX_METRES) {
          setNearby(h.label);
        }
      })
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setLooking(false);
      });
    return () => {
      cancelled = true;
    };
  }, [place.lat, place.lng, place.address]);

  const address = nearby ?? place.address;
  const title = place.name ?? address ?? "Building";
  return (
    <div className="map-place-card glass-panel" role="dialog" aria-label={title}>
      <div className="map-place-card__head">
        <div>
          <div className="map-place-card__title">{title}</div>
          {place.kind ? <div className="map-place-card__kind">{place.kind.replace(/_/g, " ")}</div> : null}
        </div>
        <button type="button" className="map-place-card__close" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      <div className="map-place-card__address">
        {address && address !== title ? address : null}
        {!address ? (looking ? "Looking up the address…" : "No address in the offline maps for this spot.") : null}
      </div>
      <div className="map-place-card__coords">
        {place.lat.toFixed(5)}, {place.lng.toFixed(5)}
      </div>
      <div className="map-place-card__actions">
        <button type="button" onClick={() => onDirections({ lat: place.lat, lng: place.lng })}>
          Directions here
        </button>
        <button type="button" className="secondary" onClick={() => onAddMarker({ lat: place.lat, lng: place.lng })}>
          Add marker
        </button>
      </div>
    </div>
  );
}
