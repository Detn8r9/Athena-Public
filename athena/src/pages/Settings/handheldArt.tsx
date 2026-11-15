/**
 * The handheld companions, animated as the handheld draws them. Frames come from the firmware
 * (devices/handheld-cyd/src/heroes.h) through tools/export_heroes.py, so the picker shows exactly
 * what is on the device: the hero's idle loop (breathing, blinking, the cape or lantern moving, and
 * now and then a little gesture).
 */

import { useEffect, useMemo, useState, type ReactElement } from "react";
import { FRAME_MS, HERO_SIZE, HEROES, PORTRAIT, type HeroFrames } from "./handheldHeroes";

type Props = {
  character: string;
  /** Screen pixels per sprite pixel. */
  scale?: number;
  /** Head only (the 16×16 head box), for small spots. */
  portrait?: boolean;
  /** Colour of the radio light (gem, flower, antenna...). Green reads as "linked". */
  radio?: string;
  /** Play the idle loop. Off for reduced motion or when many are on screen. */
  animate?: boolean;
  title?: string;
};

export function heroById(id: string): HeroFrames {
  return HEROES.find((h) => h.id === id) ?? HEROES[0];
}

function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}

/** Advances through the idle loop while mounted. */
function useTick(enabled: boolean): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!enabled || prefersReducedMotion()) return;
    const t = window.setInterval(() => setTick((n) => n + 1), FRAME_MS);
    return () => window.clearInterval(t);
  }, [enabled]);
  return tick;
}

function frameRects(hero: HeroFrames, frame: string[], radio: string, portrait: boolean): ReactElement[] {
  const x0 = portrait ? PORTRAIT.x : 0;
  const y0 = portrait ? PORTRAIT.y : 0;
  const size = portrait ? PORTRAIT.size : HERO_SIZE;
  const rects: ReactElement[] = [];
  for (let y = 0; y < size; y++) {
    const row = frame[y0 + y];
    // Merge runs of one colour into one rect to keep the SVG small.
    let x = 0;
    while (x < size) {
      const ch = row[x0 + x];
      let run = 1;
      while (x + run < size && row[x0 + x + run] === ch) run++;
      if (ch !== ".") {
        const fill = ch === "a" ? radio : hero.palette[ch];
        rects.push(<rect key={`${x},${y}`} x={x} y={y} width={run + 0.02} height={1.02} fill={fill} />);
      }
      x += run;
    }
  }
  return rects;
}

export function CompanionSprite({
  character,
  scale = 3,
  portrait = false,
  radio = "#34D399",
  animate = true,
  title,
}: Props) {
  const hero = heroById(character);
  const tick = useTick(animate);
  const frame = hero.frames[hero.sequence[tick % hero.sequence.length]];
  const rects = useMemo(() => frameRects(hero, frame, radio, portrait), [hero, frame, radio, portrait]);
  const size = portrait ? PORTRAIT.size : HERO_SIZE;
  return (
    <svg
      width={size * scale}
      height={size * scale}
      viewBox={`0 0 ${size} ${size}`}
      shapeRendering="crispEdges"
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      style={{ imageRendering: "pixelated", display: "block" }}
    >
      {rects}
    </svg>
  );
}
