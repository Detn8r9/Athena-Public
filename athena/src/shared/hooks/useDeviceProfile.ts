import { useSyncExternalStore } from "react";

/**
 * Device profile for layout decisions CSS media queries alone can't make.
 *
 * Written to <html> as data attributes so any theme's CSS can key on them:
 *   data-input = "touch" | "mouse" | "hybrid"   (primary pointer; hybrid = mouse plus a touch screen)
 *   data-form  = "phone" | "tablet" | "desktop"
 *   data-orient = "portrait" | "landscape"
 *
 * Width thresholds match the existing CSS: the phone layout (bottom tab bar, stacked header)
 * starts at max-width 768px. Prefer plain media queries in CSS (`pointer: coarse`,
 * `hover: none`, `max-width: 768px`); use these attributes only when JS or a theme needs the
 * combined answer (for example "a touch tablet in landscape").
 */

export type InputKind = "touch" | "mouse" | "hybrid";
export type FormFactor = "phone" | "tablet" | "desktop";
export type Orientation = "portrait" | "landscape";

export type DeviceProfile = {
  input: InputKind;
  form: FormFactor;
  orient: Orientation;
  width: number;
  height: number;
};

export const PHONE_MAX_WIDTH = 768;
export const TABLET_MAX_WIDTH = 1199;

export type ProfileInputs = {
  width: number;
  height: number;
  /** Primary pointer is coarse (finger). */
  coarse: boolean;
  /** Any attached pointer is coarse (touch screen present). */
  anyCoarse: boolean;
  /** Any attached pointer is fine (mouse or trackpad present). */
  anyFine: boolean;
};

export function classifyDevice({ width, height, coarse, anyCoarse, anyFine }: ProfileInputs): DeviceProfile {
  const input: InputKind = coarse ? "touch" : anyCoarse && anyFine ? "hybrid" : "mouse";
  const shortSide = Math.min(width, height);
  let form: FormFactor;
  if (width <= PHONE_MAX_WIDTH && !(input === "touch" && shortSide >= 600 && width > 700)) {
    // Narrow windows use the phone layout, except a touch tablet in portrait at ~768 wide.
    form = "phone";
  } else if (input === "touch" && shortSide < 500) {
    // A phone turned to landscape is still a phone.
    form = "phone";
  } else if (width <= TABLET_MAX_WIDTH || input === "touch") {
    form = "tablet";
  } else {
    form = "desktop";
  }
  return { input, form, orient: width >= height ? "landscape" : "portrait", width, height };
}

function mq(query: string): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(query).matches
    : false;
}

function readProfile(): DeviceProfile {
  if (typeof window === "undefined") {
    return { input: "mouse", form: "desktop", orient: "landscape", width: 1440, height: 900 };
  }
  return classifyDevice({
    width: window.innerWidth,
    height: window.innerHeight,
    coarse: mq("(pointer: coarse)"),
    anyCoarse: mq("(any-pointer: coarse)"),
    anyFine: mq("(any-pointer: fine)"),
  });
}

let current: DeviceProfile = readProfile();
const listeners = new Set<() => void>();

function applyToRoot(p: DeviceProfile): void {
  if (typeof document === "undefined") return;
  const el = document.documentElement;
  el.dataset.input = p.input;
  el.dataset.form = p.form;
  el.dataset.orient = p.orient;
}

function refresh(): void {
  const next = readProfile();
  const changed =
    next.input !== current.input ||
    next.form !== current.form ||
    next.orient !== current.orient ||
    next.width !== current.width ||
    next.height !== current.height;
  current = next;
  applyToRoot(next);
  if (changed) listeners.forEach((l) => l());
}

let installed = false;

/** Call once at startup; keeps the <html> data attributes current on resize/rotation/pointer change. */
export function installDeviceProfile(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;
  refresh();
  let raf = 0;
  const schedule = () => {
    if (raf) return;
    raf = window.requestAnimationFrame(() => {
      raf = 0;
      refresh();
    });
  };
  window.addEventListener("resize", schedule);
  window.addEventListener("orientationchange", schedule);
  if (typeof window.matchMedia === "function") {
    for (const q of ["(pointer: coarse)", "(any-pointer: coarse)", "(any-pointer: fine)"]) {
      window.matchMedia(q).addEventListener?.("change", schedule);
    }
  }
}

function subscribe(cb: () => void): () => void {
  installDeviceProfile();
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** Current device profile; re-renders when it changes. */
export function useDeviceProfile(): DeviceProfile {
  return useSyncExternalStore(subscribe, () => current, () => current);
}
