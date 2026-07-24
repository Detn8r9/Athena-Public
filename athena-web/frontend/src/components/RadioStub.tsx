export function RadioStub() {
  return (
    <div>
      <p className="stub-copy">
        Radio / mesh stub — Meshtastic and LAN P2C hooks land here without cluttering the home grid.
      </p>
      <div className="row-actions" style={{ marginTop: "0.75rem" }}>
        <button type="button" className="btn" disabled>
          Scan channels
        </button>
        <button type="button" className="btn" disabled>
          Open logbook
        </button>
      </div>
    </div>
  );
}
