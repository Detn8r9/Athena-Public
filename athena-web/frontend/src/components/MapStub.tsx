export function MapStub() {
  return (
    <div>
      <p className="stub-copy">
        Map widget stub — wire offline tiles / missions here. Drag this panel anywhere on the grid.
      </p>
      <div
        style={{
          marginTop: "0.75rem",
          height: "70%",
          minHeight: 120,
          borderRadius: 12,
          border: "1px dashed var(--line)",
          background:
            "repeating-linear-gradient(0deg, transparent, transparent 24px, rgba(143,163,152,0.08) 25px), repeating-linear-gradient(90deg, transparent, transparent 24px, rgba(143,163,152,0.08) 25px)",
          display: "grid",
          placeItems: "center",
          color: "var(--muted)",
          fontSize: "0.85rem",
        }}
      >
        Offline map canvas
      </div>
    </div>
  );
}
