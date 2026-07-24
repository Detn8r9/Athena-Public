import { LayoutConfig } from "../lib/api";

type Props = {
  layout: LayoutConfig;
  onChange: (next: LayoutConfig) => void;
};

export function LayoutSettings({ layout, onChange }: Props) {
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
      <p className="stub-copy">
        Drag widget headers to rearrange. Favorites live on the home grid; everything else stays in
        More Apps.
      </p>
    </div>
  );
}
