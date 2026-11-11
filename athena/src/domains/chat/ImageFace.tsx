import { useEffect, useMemo, useState } from "react";
import type { AthenaEmotion } from "./chatSurfacePrefs";
import { YesManFace } from "./YesManFace";

const modules = import.meta.glob("./face-assets/*.{png,webp,jpg,jpeg}", {
  eager: true,
  import: "default",
}) as Record<string, string>;

function assetUrl(filename: string): string | null {
  const hit = Object.entries(modules).find(([k]) => k.endsWith(`/face-assets/${filename}`) || k.endsWith(`\\face-assets\\${filename}`));
  return hit?.[1] ?? null;
}

const EMOTION_FILE: Record<AthenaEmotion, string[]> = {
  neutral: ["idle.png", "neutral.png"],
  happy: ["happy.png", "idle.png"],
  listening: ["listening.png", "idle.png"],
  thinking: ["thinking.png", "idle.png"],
  speaking: ["speaking-01.png", "idle.png"],
  concerned: ["concerned.png", "sad.png", "idle.png"],
  surprised: ["surprised.png", "idle.png"],
  curious: ["curious.png", "idle.png"],
};

const SPEAKING_CYCLE = [
  "speaking-01.png",
  "speaking-02.png",
  "speaking-03.png",
  "speaking-04.png",
  "speaking-05.png",
  "speaking-06.png",
];

type Props = {
  emotion: AthenaEmotion;
  mouthOpen?: number;
  className?: string;
};

/**
 * Prefers user-supplied PNGs under face-assets/; falls back to SVG YesManFace.
 */
export function ImageFace({ emotion, mouthOpen = 0, className }: Props) {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    if (emotion !== "speaking") {
      setTick(0);
      return;
    }
    const id = window.setInterval(() => setTick((t) => t + 1), 140);
    return () => window.clearInterval(id);
  }, [emotion]);

  const src = useMemo(() => {
    if (emotion === "speaking") {
      const available = SPEAKING_CYCLE.map((f) => assetUrl(f)).filter(Boolean) as string[];
      if (available.length > 0) return available[tick % available.length];
    }
    for (const f of EMOTION_FILE[emotion] ?? ["idle.png"]) {
      const u = assetUrl(f);
      if (u) return u;
    }
    return null;
  }, [emotion, tick]);

  if (!src) {
    return <YesManFace emotion={emotion} mouthOpen={mouthOpen} className={className} />;
  }

  return (
    <div
      className={`image-face image-face--${emotion}${className ? ` ${className}` : ""}`}
      data-emotion={emotion}
      aria-hidden
    >
      <img src={src} alt="" className="image-face-img" draggable={false} />
    </div>
  );
}

export function faceAssetsPresent(): boolean {
  return Object.keys(modules).length > 0;
}
