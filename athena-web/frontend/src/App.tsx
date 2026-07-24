import { useCallback, useEffect, useMemo, useState } from "react";
import GridLayout, { Layout } from "react-grid-layout";
import { api, HealthResponse, LayoutConfig } from "./lib/api";
import { CommandBar } from "./components/CommandBar";
import { AppDrawer } from "./components/AppDrawer";
import { LayoutSettings } from "./components/LayoutSettings";
import { ChatWidget } from "./components/ChatWidget";
import { ModuleDownloader } from "./components/ModuleDownloader";
import { MapStub } from "./components/MapStub";
import { RadioStub } from "./components/RadioStub";

const WIDGET_TITLES: Record<string, string> = {
  chat: "Chat",
  modules: "Modules",
  map: "Map",
  radio: "Radio",
};

export default function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [layout, setLayout] = useState<LayoutConfig | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [width, setWidth] = useState(() => Math.min(window.innerWidth - 36, 1400));
  const [status, setStatus] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [h, l] = await Promise.all([api.health(), api.getLayout()]);
    setHealth(h);
    setLayout(l);
  }, []);

  useEffect(() => {
    void refresh().catch((e) => setStatus(String(e)));
    const onResize = () => setWidth(Math.min(window.innerWidth - 36, 1400));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [refresh]);

  const onLayoutChange = (next: Layout[]) => {
    if (!layout) return;
    const widgets = next.map((n) => {
      const prev = layout.widgets.find((w) => w.i === n.i);
      return {
        i: n.i,
        x: n.x,
        y: n.y,
        w: n.w,
        h: n.h,
        minW: prev?.minW,
        minH: prev?.minH,
      };
    });
    const updated = { ...layout, widgets };
    setLayout(updated);
    void api.saveLayout(updated).catch((e) => setStatus(String(e)));
  };

  const gridLayout: Layout[] = useMemo(
    () =>
      (layout?.widgets || []).map((w) => ({
        i: w.i,
        x: w.x,
        y: w.y,
        w: w.w,
        h: w.h,
        minW: w.minW,
        minH: w.minH,
      })),
    [layout],
  );

  const runCommand = async (raw: string) => {
    const cmd = raw.trim().toLowerCase();
    if (!cmd) return;
    if (cmd === "apps" || cmd === "more apps" || cmd.startsWith("open apps")) {
      setDrawerOpen(true);
      return;
    }
    if (cmd.startsWith("install ")) {
      const id = cmd.replace("install ", "").trim();
      setStatus(`Installing app ${id}…`);
      await api.installApp(id);
      setStatus(`Installed ${id}`);
      setDrawerOpen(true);
      return;
    }
    if (cmd === "settings" || cmd === "layout") {
      setSettingsOpen(true);
      return;
    }
    setStatus(`No handler for “${raw}”. Try: apps, settings, install jellyfin`);
  };

  if (!layout) {
    return (
      <div className="app-shell">
        <div className="brand-bar">
          <div>
            <h1>Athena</h1>
            <p>Loading command center…</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <header className="brand-bar">
        <div>
          <h1>Athena</h1>
          <p>Local modular AI command center — open on your LAN</p>
        </div>
        <div className="status-pills">
          <span className={`pill ${health?.providers.preferred === "cloud" ? "on" : ""}`}>
            Route: {health?.providers.preferred ?? "…"}
          </span>
          <span className={`pill ${health?.providers.ollama ? "on" : ""}`}>Ollama</span>
          <span className={`pill ${health?.providers.internet ? "on" : ""}`}>Internet</span>
          <span className={`pill ${health?.providers.comfyui ? "on" : ""}`}>ComfyUI</span>
        </div>
      </header>

      <CommandBar
        onSubmit={runCommand}
        onOpenApps={() => setDrawerOpen(true)}
        onOpenSettings={() => setSettingsOpen((v) => !v)}
      />

      {settingsOpen ? (
        <LayoutSettings
          layout={layout}
          onChange={(next) => {
            setLayout(next);
            void api.saveLayout(next);
          }}
        />
      ) : null}

      {status ? (
        <p className="stub-copy" style={{ margin: 0 }}>
          {status}
        </p>
      ) : null}

      <GridLayout
        className="layout"
        layout={gridLayout}
        cols={layout.cols}
        rowHeight={layout.rowHeight}
        width={width}
        draggableHandle=".widget-header"
        onLayoutChange={onLayoutChange}
        compactType="vertical"
        margin={[12, 12]}
      >
        {gridLayout.map((item) => (
          <div key={item.i}>
            <section className="widget">
              <div className="widget-header">
                <h3>{WIDGET_TITLES[item.i] ?? item.i}</h3>
              </div>
              <div className="widget-body">
                {item.i === "chat" ? <ChatWidget /> : null}
                {item.i === "modules" ? <ModuleDownloader /> : null}
                {item.i === "map" ? <MapStub /> : null}
                {item.i === "radio" ? <RadioStub /> : null}
              </div>
            </section>
          </div>
        ))}
      </GridLayout>

      {drawerOpen ? (
        <AppDrawer onClose={() => setDrawerOpen(false)} maxPinned={layout.maxApps} />
      ) : null}
    </div>
  );
}
