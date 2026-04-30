import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { CircleHelp, Lock, Moon, Sun } from "lucide-react";
import { useAuth } from "./auth/AuthContext";
import type { MeshRadioIdentity } from "./MeshNetworkHubView";

function formatMeshNode(nodeId: number | undefined | null): string {
  if (nodeId == null || !Number.isFinite(nodeId) || nodeId === 0) return "—";
  return `!${Math.trunc(nodeId).toString(16).toUpperCase()}`;
}

type LockDeviceStats = {
  cpu_usage_pct: number;
  ram_used_mb: number;
  ram_total_mb: number;
  uptime_secs: number;
  load_one: number;
  logical_cpus: number;
  host_name?: string | null;
  os_name?: string | null;
};

function formatUptime(totalSecs: number): string {
  if (!Number.isFinite(totalSecs) || totalSecs < 0) return "—";
  const s = Math.floor(totalSecs);
  const days = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (days > 0) return `${days}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function formatRamMb(used: number, total: number): string {
  if (!total || total <= 0) return "—";
  if (total < 1024) return `${Math.round(used)} / ${Math.round(total)} MB`;
  const toGb = (n: number) => (n / 1024).toFixed(1);
  return `${toGb(used)} / ${toGb(total)} GB`;
}

const SPARK_W = 220;
const SPARK_H = 52;
const SPARK_PAD = 3;

function CpuSparkline({ samples }: { samples: number[] }) {
  const dPath = useMemo(() => {
    if (samples.length < 2) return "";
    const innerW = SPARK_W - SPARK_PAD * 2;
    const innerH = SPARK_H - SPARK_PAD * 2;
    let d = "";
    for (let i = 0; i < samples.length; i++) {
      const v = Math.min(100, Math.max(0, samples[i] ?? 0));
      const x = SPARK_PAD + (i / (samples.length - 1)) * innerW;
      const y = SPARK_PAD + (1 - v / 100) * innerH;
      d += i === 0 ? `M ${x.toFixed(2)} ${y.toFixed(2)}` : ` L ${x.toFixed(2)} ${y.toFixed(2)}`;
    }
    return d;
  }, [samples]);

  const areaPath = useMemo(() => {
    if (!dPath) return "";
    const innerW = SPARK_W - SPARK_PAD * 2;
    const bottom = SPARK_H - SPARK_PAD;
    return `${dPath} L ${SPARK_PAD + innerW} ${bottom} L ${SPARK_PAD} ${bottom} Z`;
  }, [dPath]);

  if (samples.length < 2) {
    return (
      <svg
        className="lock-screen-sparkline-svg"
        viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
        preserveAspectRatio="none"
        aria-hidden
      >
        <text
          x={SPARK_W / 2}
          y={SPARK_H / 2}
          textAnchor="middle"
          dominantBaseline="middle"
          className="lock-screen-sparkline-placeholder"
        >
          Collecting…
        </text>
      </svg>
    );
  }

  return (
    <svg
      className="lock-screen-sparkline-svg"
      viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
      preserveAspectRatio="none"
      aria-hidden
    >
      <path className="lock-screen-sparkline-fill" d={areaPath} />
      <path className="lock-screen-sparkline-stroke" fill="none" d={dPath} />
    </svg>
  );
}

export function LockScreen() {
  const { login, loginGuest } = useAuth();
  const [name, setName] = useState("");
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [clockLabel, setClockLabel] = useState(() =>
    new Date().toLocaleString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }),
  );
  const [meshId, setMeshId] = useState<MeshRadioIdentity | null>(null);
  const [inferReady, setInferReady] = useState<boolean | null>(null);
  const [aiAsleep, setAiAsleep] = useState(false);
  const [sleepBusy, setSleepBusy] = useState(false);
  const [deviceSnap, setDeviceSnap] = useState<LockDeviceStats | null>(null);
  const [cpuSamples, setCpuSamples] = useState<number[]>([]);
  const [loginHelpOpen, setLoginHelpOpen] = useState(false);

  const refreshAmbient = useCallback(async () => {
    try {
      const [mesh, ready, asleep] = await Promise.all([
        invoke<MeshRadioIdentity>("get_mesh_radio_identity"),
        invoke<boolean>("is_inference_ready").catch(() => false),
        invoke<boolean>("get_ai_inference_sleep"),
      ]);
      setMeshId(mesh);
      setInferReady(ready);
      setAiAsleep(asleep);
    } catch {
      setMeshId(null);
      setInferReady(null);
    }
  }, []);

  useEffect(() => {
    let ticks = 0;
    let id: ReturnType<typeof setInterval> | undefined;
    const pollDevice = async () => {
      try {
        const d = await invoke<LockDeviceStats>("get_device_stats");
        setDeviceSnap(d);
        setCpuSamples((prev) => [...prev, d.cpu_usage_pct].slice(-56));
      } catch {
        /* ignore on lock screen */
      }
      ticks += 1;
      if (ticks >= 8) {
        ticks = 0;
        void refreshAmbient();
      }
    };
    void refreshAmbient();
    void pollDevice();
    id = window.setInterval(() => void pollDevice(), 1400);
    return () => {
      if (id !== undefined) window.clearInterval(id);
    };
  }, [refreshAmbient]);

  useEffect(() => {
    const t = window.setInterval(() => {
      setClockLabel(
        new Date().toLocaleString(undefined, {
          weekday: "short",
          month: "short",
          day: "numeric",
          hour: "numeric",
          minute: "2-digit",
        }),
      );
    }, 30_000);
    return () => {
      window.clearInterval(t);
    };
  }, []);

  useEffect(() => {
    if (!loginHelpOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setLoginHelpOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [loginHelpOpen]);

  useEffect(() => {
    let un: UnlistenFn | undefined;
    void listen<{ asleep?: boolean }>("athena:ai-inference-sleep-changed", (ev) => {
      const v = ev.payload?.asleep;
      if (typeof v === "boolean") setAiAsleep(v);
    }).then((fn) => {
      un = fn;
    });
    return () => {
      void un?.();
    };
  }, []);

  useEffect(() => {
    let un: UnlistenFn | undefined;
    void listen("athena:mesh-connection", () => {
      void refreshAmbient();
    }).then((fn) => {
      un = fn;
    });
    return () => {
      void un?.();
    };
  }, [refreshAmbient]);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const n = name.trim();
    if (!n || !pin.trim()) {
      setError("Enter operator name and PIN.");
      return;
    }
    setBusy(true);
    try {
      await login(n, pin);
      setPin("");
    } catch (err) {
      setError(err != null ? String(err) : "Authentication failed.");
    } finally {
      setBusy(false);
    }
  }

  async function onGuest() {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await loginGuest();
      setPin("");
      setName("");
    } catch (err) {
      setError(err != null ? String(err) : "Guest login failed.");
    } finally {
      setBusy(false);
    }
  }

  async function toggleAiSleep() {
    setSleepBusy(true);
    try {
      const next = !aiAsleep;
      await invoke("set_ai_inference_sleep", { value: next });
      setAiAsleep(next);
    } catch {
      /* ignore */
    } finally {
      setSleepBusy(false);
    }
  }

  const meshLine =
    meshId == null
      ? "Mesh: …"
      : meshId.connected
        ? `Mesh USB: connected · ${formatMeshNode(meshId.nodeId)}`
        : "Mesh USB: not connected";

  const modelLine =
    inferReady === null ? "Local model: …" : inferReady ? "Local model: ready" : "Local model: not ready";

  const osLabel = deviceSnap?.os_name?.trim() || null;
  const ramLine =
    deviceSnap != null
      ? formatRamMb(deviceSnap.ram_used_mb, deviceSnap.ram_total_mb)
      : "—";
  const uptimeLine = deviceSnap != null ? formatUptime(deviceSnap.uptime_secs) : "—";
  const loadLine =
    deviceSnap != null && Number.isFinite(deviceSnap.load_one)
      ? deviceSnap.load_one.toFixed(2)
      : "—";
  const lastCpu =
    cpuSamples.length > 0 ? cpuSamples[cpuSamples.length - 1] : deviceSnap?.cpu_usage_pct ?? null;

  return (
    <div className="lock-screen">
      <div className="lock-screen-help-corner">
        <button
          type="button"
          className="lock-screen-help-btn"
          onClick={() => setLoginHelpOpen(true)}
          aria-expanded={loginHelpOpen}
          aria-haspopup="dialog"
          aria-controls="lock-screen-login-help-dialog"
          title="Quick help for signing in"
        >
          <CircleHelp size={20} strokeWidth={2} aria-hidden />
          <span className="lock-screen-help-btn-text">Help</span>
        </button>
      </div>

      {loginHelpOpen ? (
        <div
          className="lock-screen-help-backdrop"
          role="presentation"
          onClick={() => setLoginHelpOpen(false)}
        >
          <div
            id="lock-screen-login-help-dialog"
            className="lock-screen-help-sheet glass-panel"
            role="dialog"
            aria-modal="true"
            aria-labelledby="lock-screen-login-help-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="lock-screen-login-help-title" className="lock-screen-help-title">
              Signing in
            </h2>
            <ul className="lock-screen-help-list">
              <li>Use the <strong>operator name</strong> and <strong>PIN</strong> from your roster. Both fields are required.</li>
              <li>PIN checks run on this device only; they are not sent to a cloud login service.</li>
              <li>
                After a failed attempt, verify caps lock and spelling. Ask an administrator to reset your PIN in{" "}
                <strong>Personnel</strong> if needed.
              </li>
              <li>
                The <strong>moon / sun</strong> button (top right) toggles local AI sleep; mesh features may still run depending on
                settings.
              </li>
              <li>The status chips at the bottom show time, mesh USB, model readiness, and AI sleep state.</li>
            </ul>
            <button type="button" className="lock-screen-help-dismiss" onClick={() => setLoginHelpOpen(false)}>
              Got it
            </button>
          </div>
        </div>
      ) : null}

      <div className="lock-screen-sleep-corner">
        <button
          type="button"
          className={`lock-screen-sleep-btn${aiAsleep ? " lock-screen-sleep-btn-asleep" : ""}`}
          onClick={() => void toggleAiSleep()}
          disabled={sleepBusy}
          title={
            aiAsleep
              ? "AI is asleep (local chat paused). Meshtastic /ai and mesh auto-replies still run. Click to wake."
              : "Put local AI to sleep (mesh /ai and auto-replies still run). Click when asleep to wake."
          }
          aria-pressed={aiAsleep}
          aria-label={aiAsleep ? "Wake local AI" : "Sleep local AI"}
        >
          {aiAsleep ? <Sun size={20} strokeWidth={2} aria-hidden /> : <Moon size={20} strokeWidth={2} aria-hidden />}
        </button>
      </div>

      <div className="lock-screen-center">
        <div className="lock-screen-layout">
          <aside className="lock-screen-ambient-panel glass-panel" aria-label="System activity snapshot">
            <p className="lock-screen-ambient-panel-title">This device</p>
            <div className="lock-screen-sparkline-head">
              <span className="lock-screen-sparkline-label">CPU (live)</span>
              <span className="lock-screen-sparkline-value">
                {lastCpu != null && Number.isFinite(lastCpu) ? `${lastCpu.toFixed(0)}%` : "—"}
              </span>
            </div>
            <div className="lock-screen-sparkline-wrap">
              <CpuSparkline samples={cpuSamples} />
            </div>
            <dl className="lock-screen-details lock-screen-details-side">
              <div className="lock-screen-detail">
                <dt>System</dt>
                <dd>{osLabel ?? "—"}</dd>
              </div>
              <div className="lock-screen-detail">
                <dt>Memory</dt>
                <dd>{ramLine}</dd>
              </div>
              <div className="lock-screen-detail">
                <dt>Uptime</dt>
                <dd>{uptimeLine}</dd>
              </div>
              <div className="lock-screen-detail">
                <dt>Load (1m)</dt>
                <dd>{loadLine}</dd>
              </div>
              <div className="lock-screen-detail">
                <dt>CPUs</dt>
                <dd>{deviceSnap?.logical_cpus ?? "—"}</dd>
              </div>
              <div className="lock-screen-detail lock-screen-detail-wide">
                <dt>Mesh radio</dt>
                <dd>
                  {meshId == null
                    ? "…"
                    : meshId.connected
                      ? `${formatMeshNode(meshId.nodeId)} · ${meshId.displayName?.trim() || "USB"}`
                      : "Not connected"}
                </dd>
              </div>
            </dl>
          </aside>

          <div className="lock-screen-login-panel glass-panel">
            <div className="lock-screen-brand">
              <span className="lock-screen-icon-wrap" aria-hidden>
                <Lock size={34} strokeWidth={2} />
              </span>
              <h1 className="lock-screen-title">Login</h1>
              <p className="lock-screen-subtitle">Verification required.</p>
            </div>
            <form className="lock-screen-form" onSubmit={onSubmit}>
              <label className="lock-screen-label">
                Operator name
                <input
                  className="lock-screen-input"
                  type="text"
                  autoComplete="username"
                  value={name}
                  onChange={(ev) => setName(ev.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="lock-screen-label">
                PIN
                <input
                  className="lock-screen-input"
                  type="password"
                  inputMode="numeric"
                  autoComplete="current-password"
                  value={pin}
                  onChange={(ev) => setPin(ev.target.value)}
                  disabled={busy}
                />
              </label>
              {error ? (
                <p className="lock-screen-error" role="alert">
                  {error}
                </p>
              ) : null}
              <div style={{ display: "grid", gap: 10, marginTop: 10 }}>
                <button type="submit" className="lock-screen-submit" disabled={busy}>
                  {busy ? "Verifying…" : "Log in"}
                </button>
                <button type="button" className="secondary" disabled={busy} onClick={() => void onGuest()}>
                  Access as Guest (Emergency/Refugee)
                </button>
              </div>
            </form>
          </div>
        </div>
      </div>

      <div className="lock-screen-status-bar" aria-live="polite">
        <span className="lock-screen-status-chip">{clockLabel}</span>
        <span className="lock-screen-status-chip">{meshLine}</span>
        <span className="lock-screen-status-chip">{modelLine}</span>
        <span className="lock-screen-status-chip">
          {aiAsleep ? "Local AI: asleep (mesh prompts still run)" : "Local AI: awake"}
        </span>
      </div>
    </div>
  );
}
