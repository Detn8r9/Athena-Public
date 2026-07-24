import { useEffect, useState } from "react";
import { api, GpuStatus, LayoutConfig } from "../lib/api";

type Props = {
  layout: LayoutConfig;
  onChange: (next: LayoutConfig) => void;
};

export function LayoutSettings({ layout, onChange }: Props) {
  const [gpu, setGpu] = useState<GpuStatus | null>(null);
  const [gpuError, setGpuError] = useState<string | null>(null);

  useEffect(() => {
    void api.gpuStatus().then(setGpu).catch((e) => setGpuError(String(e)));
  }, []);

  const setOverride = async (value: boolean | null) => {
    setGpuError(null);
    try {
      setGpu(await api.setGpuOverride(value));
    } catch (e) {
      setGpuError(String(e));
    }
  };

  return (
    <div className="settings-panel">
      <strong>Layout settings</strong>
      <label>
        Grid columns
        <input
          type="number"
          min={4}
          max={24}
          value={layout.cols}
          onChange={(e) => onChange({ ...layout, cols: Number(e.target.value) || 12 })}
        />
      </label>
      <label>
        Row height (px)
        <input
          type="number"
          min={24}
          max={120}
          value={layout.rowHeight}
          onChange={(e) => onChange({ ...layout, rowHeight: Number(e.target.value) || 48 })}
        />
      </label>
      <label>
        Max pinned apps on home
        <input
          type="number"
          min={1}
          max={24}
          value={layout.maxApps}
          onChange={(e) => onChange({ ...layout, maxApps: Number(e.target.value) || 6 })}
        />
      </label>
      <label>
        Extra rows (vertical room)
        <input
          type="number"
          min={4}
          max={40}
          value={layout.rows}
          onChange={(e) => onChange({ ...layout, rows: Number(e.target.value) || 8 })}
        />
      </label>

      <strong style={{ marginTop: "0.35rem" }}>GPU acceleration</strong>
      <p className="stub-copy">
        Auto-detects Docker nvidia runtime. Override when the host has (or lacks) a compatible NVIDIA
        GPU. Whisper/Kokoro image tags and device flags follow this setting.
      </p>
      {gpu ? (
        <>
          <p className="stub-copy">
            Detected: {gpu.nvidia_runtime_available ? "yes" : "no"} · Effective:{" "}
            {gpu.effective_gpu ? "GPU" : "CPU"} · {gpu.detail}
          </p>
          <label style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
            <input
              type="checkbox"
              checked={gpu.override === true || (gpu.override === null && gpu.effective_gpu)}
              onChange={(e) => {
                if (gpu.override === null) {
                  void setOverride(e.target.checked);
                } else if (e.target.checked) {
                  void setOverride(true);
                } else {
                  void setOverride(false);
                }
              }}
            />
            This machine has an NVIDIA GPU (use GPU container variants)
          </label>
          <div className="row-actions">
            <button type="button" className="btn ghost" onClick={() => void setOverride(null)}>
              Reset to auto-detect
            </button>
          </div>
        </>
      ) : (
        <p className="stub-copy">{gpuError ?? "Loading GPU status…"}</p>
      )}

      <p className="stub-copy">
        Drag widget headers to rearrange. Favorites live on the home grid; everything else stays in
        More Apps.
      </p>
    </div>
  );
}
