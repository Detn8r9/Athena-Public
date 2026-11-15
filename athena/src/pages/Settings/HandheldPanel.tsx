import { useCallback, useEffect, useState } from "react";
import { athenaInvoke } from "../../shared/ipc/athenaIpc";
import { CompanionSprite } from "./handheldArt";
import "./handheld.css";

type Companion = { id: string; name: string; blurb: string };
type Device = {
  deviceId: string;
  ip: string;
  rssi: number;
  firmware: string;
  lastSeen: number;
  online: boolean;
};
type Overview = { character: string; characters: Companion[]; devices: Device[] };

function ago(unix: number): string {
  const s = Math.max(0, Math.floor(Date.now() / 1000) - unix);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}

function signal(rssi: number): { bars: number; label: string } {
  if (!rssi) return { bars: 0, label: "unknown" };
  if (rssi > -55) return { bars: 4, label: "excellent" };
  if (rssi > -65) return { bars: 3, label: "good" };
  if (rssi > -75) return { bars: 2, label: "fair" };
  return { bars: 1, label: "weak" };
}

/** "a1b2c3d4e5f6" → "D4:E5:F6", the part printed nowhere but easy to tell apart. */
function shortId(id: string): string {
  return (id.slice(-6).match(/../g) ?? []).join(":").toUpperCase();
}

/**
 * Settings → Me: the companion this operator's handhelds show, and the handhelds signed in as
 * them. The station keeps both; a handheld picks up a new companion on its next poll (~10 s).
 */
export function HandheldPanel() {
  const [data, setData] = useState<Overview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(() => {
    void athenaInvoke<Overview>("handheld_overview")
      .then((d) => {
        setData(d);
        setErr(null);
      })
      .catch((e) => setErr(e != null ? String(e) : "Could not load handhelds."));
  }, []);

  useEffect(() => {
    load();
    const t = window.setInterval(load, 10_000);
    return () => window.clearInterval(t);
  }, [load]);

  const choose = (id: string) => {
    if (!data || id === data.character) return;
    setBusy(id);
    void athenaInvoke<Overview>("handheld_set_character", { character: id })
      .then(setData)
      .catch((e) => setErr(String(e)))
      .finally(() => setBusy(null));
  };

  const forget = (deviceId: string) => {
    setBusy(deviceId);
    void athenaInvoke<Overview>("handheld_forget_device", { deviceId })
      .then(setData)
      .catch((e) => setErr(String(e)))
      .finally(() => setBusy(null));
  };

  const anyOnline = data?.devices.some((d) => d.online) ?? false;

  return (
    <section className="library-section handheld-panel" aria-labelledby="handheld-panel-title">
      <div className="glass-panel handheld-panel__card">
        <div>
          <p id="handheld-panel-title" className="handheld-panel__title">
            Handheld companion
          </p>
          <p className="text-muted small handheld-panel__lead">
            Your survivor on every ATHENA handheld you sign in to. Their mood and pose tell you how the compound is
            doing, and the small light they carry (gem, flower, antenna) glows while the mesh radio is linked.
          </p>
        </div>

        <div className="handheld-companions" role="radiogroup" aria-label="Handheld companion">
          {(data?.characters ?? []).map((c) => {
            const selected = c.id === data?.character;
            return (
              <button
                key={c.id}
                type="button"
                role="radio"
                aria-checked={selected}
                className={["handheld-companion", selected ? "handheld-companion--selected" : ""].join(" ")}
                disabled={busy !== null}
                onClick={() => choose(c.id)}
              >
                <span className="handheld-companion__stage">
                  <CompanionSprite character={c.id} scale={3} />
                </span>
                <span className="handheld-companion__name">
                  {c.name}
                  {selected ? <span className="handheld-companion__badge">Yours</span> : null}
                </span>
                <span className="handheld-companion__blurb">{c.blurb}</span>
              </button>
            );
          })}
        </div>

        <div>
          <p className="handheld-panel__subtitle">
            Your handhelds
            {data ? <span className="text-muted small"> · {data.devices.length}</span> : null}
          </p>
          {data && data.devices.length === 0 ? (
            <div className="handheld-empty">
              <CompanionSprite character={data.character} scale={2} portrait radio="#8B9BB4" />
              <p className="text-muted small" style={{ margin: 0 }}>
                No handheld has signed in as you yet. Turn on <strong>Allow LAN access</strong> in General, then sign in
                on the handheld with your PIN. It shows up here within a few seconds.
              </p>
            </div>
          ) : null}
          <ul className="handheld-devices">
            {(data?.devices ?? []).map((d) => {
              const sig = signal(d.rssi);
              return (
                <li key={d.deviceId} className="handheld-device">
                  <span className="handheld-device__pet">
                    <CompanionSprite
                      character={data?.character ?? "wren"}
                      scale={2}
                      portrait
                      radio={d.online ? "#34D399" : "#8B9BB4"}
                    />
                  </span>
                  <span className="handheld-device__main">
                    <span className="handheld-device__name">
                      Handheld {shortId(d.deviceId)}
                      <span
                        className={["handheld-device__status", d.online ? "handheld-device__status--on" : ""].join(" ")}
                      >
                        {d.online ? "Online" : `Last seen ${ago(d.lastSeen)}`}
                      </span>
                    </span>
                    <span className="text-muted small">
                      {d.ip || "IP unknown"}
                      {" · "}
                      <span className="handheld-bars" aria-label={`Wi-Fi signal ${sig.label}`}>
                        {[1, 2, 3, 4].map((b) => (
                          <i key={b} className={b <= sig.bars ? "on" : ""} style={{ height: 3 + b * 2 }} />
                        ))}
                      </span>{" "}
                      {sig.label}
                      {d.firmware ? ` · firmware ${d.firmware}` : ""}
                    </span>
                  </span>
                  <button
                    type="button"
                    className="secondary handheld-device__forget"
                    disabled={busy !== null}
                    onClick={() => forget(d.deviceId)}
                    title="Remove from your profile. It comes back if it signs in again."
                  >
                    Forget
                  </button>
                </li>
              );
            })}
          </ul>
          {data && data.devices.length > 0 && !anyOnline ? (
            <p className="text-muted small" style={{ margin: "0.4rem 0 0" }}>
              None online right now. A handheld counts as online while it has checked in within the last minute.
            </p>
          ) : null}
        </div>

        {err ? (
          <p className="quartermaster-hub-err" role="alert" style={{ margin: 0 }}>
            {err}
          </p>
        ) : null}
      </div>
    </section>
  );
}
