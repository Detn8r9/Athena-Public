import type { AthenaEmotion } from "./chatSurfacePrefs";

type Props = {
  emotion: AthenaEmotion;
  /** 0–1 mouth open amount while speaking */
  mouthOpen?: number;
  className?: string;
};

/**
 * Yes Man–inspired holographic face (original SVG art — Fallout aesthetic homage).
 */
export function YesManFace({ emotion, mouthOpen = 0, className }: Props) {
  const eyeShape = (() => {
    switch (emotion) {
      case "happy":
        return { ry: 4, cy: 78 };
      case "concerned":
        return { ry: 7, cy: 82 };
      case "surprised":
        return { ry: 14, cy: 78 };
      case "thinking":
        return { ry: 6, cy: 80 };
      case "listening":
        return { ry: 9, cy: 78 };
      case "curious":
        return { ry: 10, cy: 76 };
      default:
        return { ry: 8, cy: 78 };
    }
  })();

  const browTilt = emotion === "concerned" ? 8 : emotion === "curious" ? -6 : emotion === "thinking" ? 4 : 0;
  const mouthH = emotion === "speaking" ? 6 + mouthOpen * 22 : emotion === "happy" ? 10 : emotion === "surprised" ? 16 : 5;
  const glow =
    emotion === "speaking"
      ? 1
      : emotion === "listening"
        ? 0.85
        : emotion === "thinking"
          ? 0.7
          : 0.55;

  return (
    <div
      className={`yesman-face yesman-face--${emotion}${className ? ` ${className}` : ""}`}
      data-emotion={emotion}
      style={{ ["--yesman-glow" as string]: String(glow) }}
      aria-hidden
    >
      <svg viewBox="0 0 200 240" className="yesman-face-svg" role="img">
        <defs>
          <radialGradient id="yesmanScreen" cx="50%" cy="40%" r="65%">
            <stop offset="0%" stopColor="#ffe566" stopOpacity="0.95" />
            <stop offset="55%" stopColor="#f0c040" stopOpacity="0.55" />
            <stop offset="100%" stopColor="#1a1408" stopOpacity="0.9" />
          </radialGradient>
          <filter id="yesmanScan">
            <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="1" result="noise" />
            <feColorMatrix type="saturate" values="0" />
          </filter>
        </defs>

        {/* Screen bezel */}
        <rect x="18" y="22" width="164" height="196" rx="28" fill="#1c1810" stroke="#6b5a2a" strokeWidth="4" />
        <rect x="28" y="34" width="144" height="172" rx="20" fill="url(#yesmanScreen)" />

        {/* Scanlines */}
        <g opacity="0.18" className="yesman-scanlines">
          {Array.from({ length: 22 }).map((_, i) => (
            <line key={i} x1="32" x2="168" y1={42 + i * 8} y2={42 + i * 8} stroke="#3a2e10" strokeWidth="1.5" />
          ))}
        </g>

        {/* Brows */}
        <path
          d={`M58 58 q22 ${-10 + browTilt} 44 0`}
          fill="none"
          stroke="#2a220e"
          strokeWidth="5"
          strokeLinecap="round"
          className="yesman-brow"
        />
        <path
          d={`M98 58 q22 ${-10 - browTilt} 44 0`}
          fill="none"
          stroke="#2a220e"
          strokeWidth="5"
          strokeLinecap="round"
          className="yesman-brow"
        />

        {/* Eyes */}
        <ellipse
          className="yesman-eye yesman-eye--left"
          cx="78"
          cy={eyeShape.cy}
          rx="16"
          ry={eyeShape.ry}
          fill="#1a1408"
        />
        <ellipse
          className="yesman-eye yesman-eye--right"
          cx="122"
          cy={eyeShape.cy}
          rx="16"
          ry={eyeShape.ry}
          fill="#1a1408"
        />
        {(emotion === "happy" || emotion === "speaking" || emotion === "curious") && (
          <>
            <circle cx="78" cy={eyeShape.cy} r="3.5" fill="#ffe566" className="yesman-pupil" />
            <circle cx="122" cy={eyeShape.cy} r="3.5" fill="#ffe566" className="yesman-pupil" />
          </>
        )}

        {/* Nose hint */}
        <path d="M100 96 l-6 18 h12 z" fill="#2a220e" opacity="0.35" />

        {/* Mouth */}
        <ellipse
          className="yesman-mouth"
          cx="100"
          cy="148"
          rx={emotion === "happy" ? 28 : 22}
          ry={mouthH}
          fill="#1a140e"
        />
        {emotion === "speaking" || emotion === "happy" ? (
          <ellipse cx="100" cy={148 + mouthH * 0.15} rx="10" ry={Math.max(2, mouthH * 0.35)} fill="#5c3a12" opacity="0.7" />
        ) : null}

        {/* Cheek glow */}
        <ellipse cx="52" cy="120" rx="10" ry="14" fill="#ff9f43" opacity="0.18" className="yesman-cheek" />
        <ellipse cx="148" cy="120" rx="10" ry="14" fill="#ff9f43" opacity="0.18" className="yesman-cheek" />

        {/* Status LEDs */}
        <circle cx="44" cy="48" r="4" className="yesman-led" fill={emotion === "listening" ? "#3dff9a" : "#5a4a20"} />
        <circle cx="156" cy="48" r="4" className="yesman-led" fill={emotion === "thinking" ? "#5eb0ff" : "#5a4a20"} />
      </svg>
      <div className="yesman-face-caption">{emotion}</div>
    </div>
  );
}
