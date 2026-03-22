import { useCallback, useEffect, useRef, useState } from "react";
import {
  clamp,
  hexToHsv,
  hsvToHex,
  hsvToRgb,
  parseHex,
  rgbToHex,
  rgbToHsv,
} from "./colorUtils";

type Props = {
  value: string;
  onChange: (hex: string) => void;
  disabled?: boolean;
  label?: string;
};

export function PhotoshopColorPicker({ value, onChange, disabled, label }: Props) {
  const [open, setOpen] = useState(false);
  const [h, setH] = useState(210);
  const [s, setS] = useState(0.8);
  const [v, setV] = useState(0.5);
  const svRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);
  const hRef = useRef(h);
  const sRef = useRef(s);
  const vRef = useRef(v);
  hRef.current = h;
  sRef.current = s;
  vRef.current = v;

  const syncFromHex = useCallback((hex: string) => {
    const parsed = parseHex(hex);
    if (parsed) {
      const hsv = rgbToHsv(parsed.r, parsed.g, parsed.b);
      setH(hsv.h);
      setS(hsv.s);
      setV(hsv.v);
    }
  }, []);

  useEffect(() => {
    syncFromHex(value);
  }, [value, syncFromHex]);

  const commitHsv = useCallback(
    (nh: number, ns: number, nv: number) => {
      setH(nh);
      setS(ns);
      setV(nv);
      onChange(hsvToHex(nh, ns, nv));
    },
    [onChange]
  );

  const onSvPointer = useCallback(
    (clientX: number, clientY: number) => {
      const el = svRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const x = clamp((clientX - rect.left) / rect.width, 0, 1);
      const y = clamp((clientY - rect.top) / rect.height, 0, 1);
      commitHsv(hRef.current, x, 1 - y);
    },
    [commitHsv]
  );

  const onHuePointer = useCallback(
    (clientY: number) => {
      const el = hueRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const y = clamp((clientY - rect.top) / rect.height, 0, 1);
      commitHsv((1 - y) * 360, sRef.current, vRef.current);
    },
    [commitHsv]
  );

  function handleSvPointerDown(e: React.PointerEvent) {
    e.preventDefault();
    const move = (ev: PointerEvent) => onSvPointer(ev.clientX, ev.clientY);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    onSvPointer(e.clientX, e.clientY);
  }

  function handleHuePointerDown(e: React.PointerEvent) {
    e.preventDefault();
    const move = (ev: PointerEvent) => onHuePointer(ev.clientY);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    onHuePointer(e.clientY);
  }

  const rgb = hsvToRgb(h, s, v);
  const hexFromState = rgbToHex(rgb.r, rgb.g, rgb.b);
  const pureHue = hsvToHex(h, 1, 1);

  return (
    <div className={`ps-color-picker ${disabled ? "ps-color-picker-disabled" : ""}`}>
      <button
        type="button"
        className="ps-color-picker-toggle"
        onClick={() => !disabled && setOpen((o) => !o)}
        disabled={disabled}
        aria-expanded={open}
        aria-label={label ? `${label} — advanced color picker` : "Advanced color picker"}
      >
        <span className="ps-color-picker-toggle-swatch" style={{ background: value }} aria-hidden />
        <span className="ps-color-picker-toggle-text">Advanced picker</span>
        <span className="ps-color-picker-chevron" aria-hidden>
          {open ? "▾" : "▸"}
        </span>
      </button>
      {open && !disabled && (
        <div className="ps-color-picker-panel" role="dialog" aria-label="HSV color picker">
          <div className="ps-color-picker-row">
            <div
              ref={svRef}
              className="ps-sv-square"
              style={{
                backgroundColor: pureHue,
                backgroundImage:
                  "linear-gradient(to right, #ffffff, transparent), linear-gradient(to top, #000000, transparent)",
              }}
              onPointerDown={handleSvPointerDown}
            >
              <div
                className="ps-sv-cursor"
                style={{
                  left: `${s * 100}%`,
                  top: `${(1 - v) * 100}%`,
                }}
              />
            </div>
            <div
              ref={hueRef}
              className="ps-hue-strip"
              style={{
                background:
                  "linear-gradient(to bottom, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)",
              }}
              onPointerDown={handleHuePointerDown}
            >
              <div
                className="ps-hue-cursor"
                style={{ top: `${(1 - h / 360) * 100}%` }}
              />
            </div>
          </div>
          <div className="ps-color-picker-inputs">
            <label className="ps-color-picker-field">
              <span>Hex</span>
              <input
                type="text"
                className="settings-input ps-color-input-mono"
                value={hexFromState}
                onChange={(e) => {
                  const raw = e.target.value.trim();
                  if (/^#[0-9a-fA-F]{6}$/.test(raw)) {
                    onChange(raw);
                    const hsv = hexToHsv(raw);
                    if (hsv) {
                      setH(hsv.h);
                      setS(hsv.s);
                      setV(hsv.v);
                    }
                  }
                }}
                onBlur={() => onChange(hexFromState)}
                spellCheck={false}
              />
            </label>
            <div className="ps-color-picker-rgb">
              <label>
                R
                <input
                  type="number"
                  min={0}
                  max={255}
                  className="settings-input ps-color-input-mono"
                  value={rgb.r}
                  onChange={(e) => {
                    const r = clamp(Number(e.target.value), 0, 255);
                    const hsv = rgbToHsv(r, rgb.g, rgb.b);
                    commitHsv(hsv.h, hsv.s, hsv.v);
                  }}
                />
              </label>
              <label>
                G
                <input
                  type="number"
                  min={0}
                  max={255}
                  className="settings-input ps-color-input-mono"
                  value={rgb.g}
                  onChange={(e) => {
                    const g = clamp(Number(e.target.value), 0, 255);
                    const hsv = rgbToHsv(rgb.r, g, rgb.b);
                    commitHsv(hsv.h, hsv.s, hsv.v);
                  }}
                />
              </label>
              <label>
                B
                <input
                  type="number"
                  min={0}
                  max={255}
                  className="settings-input ps-color-input-mono"
                  value={rgb.b}
                  onChange={(e) => {
                    const b = clamp(Number(e.target.value), 0, 255);
                    const hsv = rgbToHsv(rgb.r, rgb.g, b);
                    commitHsv(hsv.h, hsv.s, hsv.v);
                  }}
                />
              </label>
            </div>
          </div>
          <button type="button" className="ps-color-picker-done secondary" onClick={() => setOpen(false)}>
            Done
          </button>
        </div>
      )}
    </div>
  );
}
