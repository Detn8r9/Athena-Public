import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePageVisible } from "./usePageVisible";
import type { AppView } from "./appView";
import type { ReactNode } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  Home,
  Library,
  Map,
  MessageSquare,
  Radio,
  Settings,
} from "lucide-react";

interface SystemStatsPayload {
  total_memory: number;
  used_memory: number;
  cpu_usage: number;
}

interface RadioLogEntry {
  id: string;
  direction: string;
  message: string;
  timestamp: string;
}

export type DashboardDestination = AppView | "library" | "settings";

interface DeviceStatsPayload {
  cpu_usage_pct: number;
  ram_used_mb: number;
  ram_total_mb: number;
  swap_used_mb: number;
  swap_total_mb: number;
  disk_used_gb: number;
  disk_total_gb: number;
  net_rx_bytes: number;
  net_tx_bytes: number;
  disk_read_bytes_total: number;
  disk_write_bytes_total: number;
  cpu_temp_c?: number | null;
  gpu_usage_pct?: number | null;
  battery_pct?: number | null;
  battery_charging?: boolean | null;
  battery_time_to_empty_minutes?: number | null;
}

interface DashboardViewProps {
  onNavigate: (destination: DashboardDestination) => void;
  onOpenTriage: () => void;
}

function LauncherIcon({ children }: { children: ReactNode }) {
  return (
    <span
      className="glass-panel"
      style={{
        width: "2.3rem",
        height: "2.3rem",
        display: "grid",
        placeItems: "center",
        borderRadius: "0.75rem",
        color: "var(--primary)",
        background: "color-mix(in srgb, var(--surface-solid) 84%, transparent)",
      }}
      aria-hidden
    >
      {children}
    </span>
  );
}

const LAUNCHER_ITEMS: Array<{ key: DashboardDestination; label: string; icon: ReactNode }> = [
  {
    key: "home",
    label: "Home",
    icon: (
      <LauncherIcon>
        <Home size={20} strokeWidth={2} aria-hidden />
      </LauncherIcon>
    ),
  },
  {
    key: "chat",
    label: "Chat",
    icon: (
      <LauncherIcon>
        <MessageSquare size={20} strokeWidth={2} aria-hidden />
      </LauncherIcon>
    ),
  },
  {
    key: "map",
    label: "Tactical Map",
    icon: (
      <LauncherIcon>
        <Map size={20} strokeWidth={2} aria-hidden />
      </LauncherIcon>
    ),
  },
  {
    key: "library",
    label: "Library",
    icon: (
      <LauncherIcon>
        <Library size={20} strokeWidth={2} aria-hidden />
      </LauncherIcon>
    ),
  },
  {
    key: "radio",
    label: "Comms",
    icon: (
      <LauncherIcon>
        <Radio size={20} strokeWidth={2} aria-hidden />
      </LauncherIcon>
    ),
  },
  {
    key: "settings",
    label: "Settings",
    icon: (
      <LauncherIcon>
        <Settings size={20} strokeWidth={2} aria-hidden />
      </LauncherIcon>
    ),
  },
];

function ThinBar({ percent }: { percent: number }) {
  const clamped = Math.max(0, Math.min(100, percent));
  return (
    <div className="glass-panel" style={{ height: "6px", padding: 0, overflow: "hidden", borderRadius: "999px" }} aria-hidden>
      <div
        style={{
          width: `${clamped}%`,
          height: "100%",
          background: "color-mix(in srgb, var(--primary) 70%, var(--tertiary))",
          transition: "width 180ms ease-out",
        }}
      />
    </div>
  );
}

export function DashboardView({ onNavigate, onOpenTriage }: DashboardViewProps) {
  const pageVisible = usePageVisible();
  const [stats, setStats] = useState<SystemStatsPayload | null>(null);
  const [statsErr, setStatsErr] = useState<string | null>(null);
  const [logs, setLogs] = useState<RadioLogEntry[]>([]);
  const [systemExpanded, setSystemExpanded] = useState(false);
  const [detailStats, setDetailStats] = useState<DeviceStatsPayload | null>(null);
  const [detailErr, setDetailErr] = useState<string | null>(null);
  const prevNetRef = useRef<{ rx: number; tx: number; t: number } | null>(null);
  const prevDiskRef = useRef<{ read: number; write: number; t: number } | null>(null);
  const [netSpeedRxKbps, setNetSpeedRxKbps] = useState<number | null>(null);
  const [netSpeedTxKbps, setNetSpeedTxKbps] = useState<number | null>(null);
  const [diskSpeedReadMbps, setDiskSpeedReadMbps] = useState<number | null>(null);
  const [diskSpeedWriteMbps, setDiskSpeedWriteMbps] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setInterval> | null = null;
    const load = async () => {
      try {
        const payload = await invoke<SystemStatsPayload>("get_system_stats");
        if (!cancelled) {
          setStats(payload);
          setStatsErr(null);
        }
      } catch (e) {
        if (!cancelled) setStatsErr(e != null ? String(e) : "Telemetry unavailable.");
      }
    };
    void load();
    const pollMs = pageVisible ? 5000 : 60000;
    timer = setInterval(() => void load(), pollMs);
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
  }, [pageVisible]);

  useEffect(() => {
    let cancelled = false;
    const loadRecentComms = async () => {
      try {
        const recentLogs = await invoke<RadioLogEntry[]>("get_recent_radio_logs");
        if (cancelled) return;
        const logList = Array.isArray(recentLogs) ? recentLogs : [];
        setLogs(logList.slice(Math.max(0, logList.length - 5)));
      } catch {
        if (!cancelled) setLogs([]);
      }
    };
    void loadRecentComms();
    const pollMs = pageVisible ? 8000 : 120000;
    const timer = setInterval(() => void loadRecentComms(), pollMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [pageVisible]);

  useEffect(() => {
    if (!systemExpanded) {
      setDetailStats(null);
      setDetailErr(null);
      prevNetRef.current = null;
      prevDiskRef.current = null;
      setNetSpeedRxKbps(null);
      setNetSpeedTxKbps(null);
      setDiskSpeedReadMbps(null);
      setDiskSpeedWriteMbps(null);
      return;
    }
    let cancelled = false;
    const tick = async () => {
      try {
        const s = await invoke<DeviceStatsPayload>("get_device_stats");
        if (cancelled) return;
        setDetailErr(null);
        const now = Date.now();
        const prev = prevNetRef.current;
        if (prev !== null && now > prev.t) {
          const dtSec = (now - prev.t) / 1000;
          if (dtSec > 0) {
            setNetSpeedRxKbps((s.net_rx_bytes - prev.rx) / 1024 / dtSec);
            setNetSpeedTxKbps((s.net_tx_bytes - prev.tx) / 1024 / dtSec);
          }
        }
        prevNetRef.current = { rx: s.net_rx_bytes, tx: s.net_tx_bytes, t: now };

        const prevD = prevDiskRef.current;
        if (prevD !== null && now > prevD.t) {
          const dtSec = (now - prevD.t) / 1000;
          if (dtSec > 0) {
            const dr = s.disk_read_bytes_total - prevD.read;
            const dw = s.disk_write_bytes_total - prevD.write;
            if (dr >= 0 && dw >= 0) {
              setDiskSpeedReadMbps(dr / (1024 * 1024) / dtSec);
              setDiskSpeedWriteMbps(dw / (1024 * 1024) / dtSec);
            }
          }
        }
        prevDiskRef.current = {
          read: s.disk_read_bytes_total,
          write: s.disk_write_bytes_total,
          t: now,
        };
        setDetailStats(s);
      } catch (e) {
        if (!cancelled) {
          setDetailErr(e != null ? String(e) : "Could not load device stats.");
          setDetailStats(null);
        }
      }
    };
    void tick();
    const pollMs = pageVisible ? 3000 : 30000;
    const id = setInterval(() => void tick(), pollMs);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [systemExpanded, pageVisible]);

  const memoryPct = useMemo(() => {
    if (!stats || stats.total_memory <= 0) return 0;
    return (stats.used_memory / stats.total_memory) * 100;
  }, [stats]);

  const cpuPct = useMemo(() => (stats ? Math.max(0, Math.min(100, stats.cpu_usage)) : 0), [stats]);

  /** Match horizontal inset to grid gap so edge→first tile spacing equals tile gaps. */
  const dashGap = "0.9rem";

  return (
    <div
      style={{
        display: "grid",
        gap: dashGap,
        width: "100%",
        boxSizing: "border-box",
        padding: `0 ${dashGap}`,
      }}
    >
      <section className="glass-panel" style={{ padding: "0.6rem 0.8rem" }}>
        {statsErr ? (
          <p className="error" role="alert" style={{ margin: 0 }}>
            {statsErr}
          </p>
        ) : (
          <>
            <button
              type="button"
              onClick={() => setSystemExpanded((v) => !v)}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "0.8rem",
                flexWrap: "wrap",
                width: "100%",
                background: "none",
                border: "none",
                padding: 0,
                cursor: "pointer",
                color: "inherit",
                textAlign: "left",
              }}
              aria-expanded={systemExpanded}
              aria-label={systemExpanded ? "Collapse system details" : "Expand system details"}
            >
              <span style={{ color: "var(--primary)", display: "inline-flex", alignItems: "center", gap: "0.35rem" }}>
                System Status
                {systemExpanded ? <ChevronUp size={16} aria-hidden /> : <ChevronDown size={16} aria-hidden />}
              </span>
              <span className="text-muted" style={{ fontSize: "0.72rem" }}>
                {systemExpanded ? "Hide details" : "Tap for disk, network, battery"}
              </span>
            </button>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "0.8rem", flexWrap: "wrap", marginTop: "0.45rem" }}>
              <div style={{ display: "grid", gap: "0.25rem", minWidth: "240px", flex: 1 }}>
                <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.82rem" }}>
                  <span>CPU {cpuPct.toFixed(1)}%</span>
                  <span>RAM {stats ? `${stats.used_memory.toFixed(1)} / ${stats.total_memory.toFixed(1)} GB` : "—"}</span>
                </div>
                <ThinBar percent={cpuPct} />
                <ThinBar percent={memoryPct} />
              </div>
            </div>
            {systemExpanded ? (
              <div className="dashboard-system-expanded-grid">
                {detailErr ? (
                  <p className="error" role="alert" style={{ margin: 0 }}>
                    {detailErr}
                  </p>
                ) : detailStats ? (
                  <>
                    <div className="dashboard-system-metric">
                      <span>CPU (telemetry)</span>
                      <span>{detailStats.cpu_usage_pct.toFixed(1)}%</span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>RAM</span>
                      <span>
                        {detailStats.ram_total_mb >= 1024
                          ? `${(detailStats.ram_used_mb / 1024).toFixed(2)} / ${(detailStats.ram_total_mb / 1024).toFixed(2)} GB`
                          : `${detailStats.ram_used_mb} / ${detailStats.ram_total_mb} MB`}
                      </span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Swap</span>
                      <span>
                        {detailStats.swap_total_mb > 0
                          ? `${(detailStats.swap_used_mb / 1024).toFixed(2)} / ${(detailStats.swap_total_mb / 1024).toFixed(2)} GB`
                          : "—"}
                      </span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Storage</span>
                      <span>
                        {detailStats.disk_total_gb > 0
                          ? `${detailStats.disk_used_gb.toFixed(1)} / ${detailStats.disk_total_gb.toFixed(1)} GB`
                          : "—"}
                      </span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Disk read</span>
                      <span>
                        {diskSpeedReadMbps != null && diskSpeedReadMbps > 0 ? `${diskSpeedReadMbps.toFixed(2)} MB/s` : "—"}
                      </span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Disk write</span>
                      <span>
                        {diskSpeedWriteMbps != null && diskSpeedWriteMbps > 0 ? `${diskSpeedWriteMbps.toFixed(2)} MB/s` : "—"}
                      </span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Network ↓</span>
                      <span>{netSpeedRxKbps != null ? `${netSpeedRxKbps.toFixed(0)} kB/s` : "—"}</span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Network ↑</span>
                      <span>{netSpeedTxKbps != null ? `${netSpeedTxKbps.toFixed(0)} kB/s` : "—"}</span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>Battery</span>
                      <span>
                        {detailStats.battery_pct != null && detailStats.battery_pct !== undefined
                          ? `${Math.round(detailStats.battery_pct)}%${
                              detailStats.battery_charging ? " · charging / AC" : ""
                            }${
                              detailStats.battery_time_to_empty_minutes != null &&
                              detailStats.battery_time_to_empty_minutes !== undefined &&
                              detailStats.battery_charging !== true
                                ? ` · ~${detailStats.battery_time_to_empty_minutes} min to 0%`
                                : ""
                            }`
                          : "—"}
                      </span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>GPU</span>
                      <span>{detailStats.gpu_usage_pct != null ? `${Math.round(detailStats.gpu_usage_pct)}%` : "—"}</span>
                    </div>
                    <div className="dashboard-system-metric">
                      <span>CPU temp</span>
                      <span>{detailStats.cpu_temp_c != null ? `${Math.round(detailStats.cpu_temp_c)}°C` : "—"}</span>
                    </div>
                  </>
                ) : (
                  <p className="text-muted" style={{ margin: 0 }}>
                    Loading…
                  </p>
                )}
              </div>
            ) : null}
          </>
        )}
      </section>

      <button
        type="button"
        className="dashboard-triage-tile glass-panel"
        data-triage-trigger
        onClick={() => onOpenTriage()}
      >
        <span
          className="glass-panel"
          style={{
            width: "2.3rem",
            height: "2.3rem",
            display: "grid",
            placeItems: "center",
            borderRadius: "0.75rem",
            color: "#fff",
            background: "rgba(0,0,0,0.2)",
          }}
          aria-hidden
        >
          <AlertTriangle size={22} strokeWidth={2.25} />
        </span>
        <span style={{ display: "grid", gap: "0.15rem", textAlign: "left" }}>
          <span className="dashboard-triage-label" style={{ color: "#fff" }}>
            Triage
          </span>
          <span className="dashboard-triage-sub" style={{ color: "rgba(255,255,255,0.92)" }}>
            Emergency actions &amp; medical quick reference
          </span>
        </span>
      </button>

      <div className="dashboard-launcher-row">
        <section
          className="glass-panel dashboard-recent-comms"
          style={{ padding: "0.8rem" }}
        >
          <h3 style={{ margin: "0 0 0.65rem", color: "var(--primary)" }}>Recent Comms</h3>
          {logs.length === 0 ? (
            <p className="text-muted" style={{ margin: 0 }}>
              No recent radio traffic.
            </p>
          ) : (
            <div style={{ display: "grid", gap: "0.55rem" }}>
              {logs.map((log) => (
                <div key={log.id} className="glass-panel" style={{ padding: "0.55rem 0.7rem", borderRadius: "10px" }}>
                  <div className="text-muted" style={{ fontSize: "0.78rem" }}>
                    {new Date(log.timestamp).toLocaleString()} · {log.direction}
                  </div>
                  <div style={{ marginTop: "0.2rem", wordBreak: "break-word" }}>{log.message}</div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section
          className="dashboard-app-launcher"
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))",
            gap: "0.9rem",
            alignContent: "start",
          }}
        >
          {LAUNCHER_ITEMS.map((item) => (
            <button
              key={item.key}
              type="button"
              className="glass-panel"
              onClick={() => onNavigate(item.key)}
              style={{
                aspectRatio: "1 / 1",
                border: "none",
                display: "grid",
                placeItems: "center",
                gap: "0.3rem",
                cursor: "pointer",
                background: "color-mix(in srgb, var(--surface-solid) 82%, transparent)",
              }}
              aria-label={`Open ${item.label}`}
            >
              {item.icon}
              <span style={{ fontSize: "0.9rem", color: "var(--primary)" }}>{item.label}</span>
            </button>
          ))}
        </section>
      </div>
    </div>
  );
}
