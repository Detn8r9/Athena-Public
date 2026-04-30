import { useEffect, useState } from "react";
import {
  MAP_MARKER_CATEGORIES,
  normalizeMapMarkerCategory,
  type MapMarkerCategory,
} from "./mapMarkerTypes";

export function MapMarkerPlacementModal({
  open,
  latLng,
  onClose,
  onSave,
}: {
  open: boolean;
  latLng: { lat: number; lng: number } | null;
  onClose: () => void;
  onSave: (payload: { title: string; description: string; category: MapMarkerCategory }) => void | Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [category, setCategory] = useState<MapMarkerCategory>("hazard");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setTitle("");
      setDescription("");
      setCategory("hazard");
      setSaving(false);
      setError(null);
    }
  }, [open]);

  useEffect(() => {
    if (open && latLng) {
      setTitle("");
      setDescription("");
      setCategory("hazard");
      setError(null);
    }
  }, [open, latLng?.lat, latLng?.lng]);

  if (!open || latLng === null) {
    return null;
  }

  return (
    <div className="modal-backdrop map-marker-modal-backdrop" onClick={onClose}>
      <div
        className="library-modal glass-panel map-marker-placement-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="map-marker-modal-title"
      >
        <div className="library-header">
          <h2 id="map-marker-modal-title">Add map marker</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="map-marker-modal-body">
          <label className="map-marker-modal-label">
            Title
            <input
              type="text"
              className="map-marker-modal-input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. Rally point"
              autoFocus
            />
          </label>
          <label className="map-marker-modal-label">
            Category
            <select
              className="map-marker-modal-input"
              value={category}
              onChange={(e) => setCategory(normalizeMapMarkerCategory(e.target.value))}
            >
              {MAP_MARKER_CATEGORIES.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
          <label className="map-marker-modal-label">
            Description (optional)
            <textarea
              className="map-marker-modal-textarea"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              placeholder="Notes…"
            />
          </label>
          {error ? (
            <p className="error" role="alert">
              {error}
            </p>
          ) : null}
          <div className="map-marker-modal-actions">
            <button type="button" className="secondary" onClick={onClose} disabled={saving}>
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                const t = title.trim();
                if (!t) {
                  setError("Title is required.");
                  return;
                }
                setError(null);
                setSaving(true);
                void Promise.resolve(
                  onSave({
                    title: t,
                    description: description.trim(),
                    category,
                  }),
                )
                  .then(() => {
                    onClose();
                  })
                  .catch((err: unknown) => {
                    setError(err != null ? String(err) : "Could not save marker.");
                  })
                  .finally(() => {
                    setSaving(false);
                  });
              }}
              disabled={saving || !title.trim()}
            >
              {saving ? "Saving…" : "Save marker"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
