import type { SVGProps } from "react";

export type CommsHubIconProps = Omit<SVGProps<SVGSVGElement>, "width" | "height"> & {
  size?: number;
  strokeWidth?: number;
};

/**
 * Studio condenser microphone with left/right level bars — Communications Hub.
 * Stroke-only, `currentColor`, sized to match Lucide header icons.
 */
export function CommsHubIcon({ size = 24, strokeWidth = 2, className, ...props }: CommsHubIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      {...props}
    >
      {/* Left waveform / meter */}
      <path d="M2.4 14.2V11.8M3.85 15.4V9.6M5.3 16.2V8.8M6.75 15.4V9.6M8.2 14.2V11.8" />
      {/* Right waveform / meter */}
      <path d="M15.8 14.2V11.8M17.25 15.4V9.6M18.7 16.2V8.8M20.15 15.4V9.6M21.6 14.2V11.8" />
      {/* Mesh grille (studio head) */}
      <ellipse cx="12" cy="7.35" rx="3.55" ry="3.95" />
      <path d="M8.45 7.35h7.1M8.65 5.55h6.7M8.65 9.15h6.7M8.9 6.45h6.2M8.9 8.25h6.2" />
      {/* Capsule below grille */}
      <path d="M9.35 11.15h5.3a0.85 0.85 0 0 1 0.85 0.85v3.35a1.1 1.1 0 0 1-1.1 1.1h-4.8a1.1 1.1 0 0 1-1.1-1.1v-3.35a0.85 0.85 0 0 1 0.85-0.85z" />
      <line x1="10.15" y1="12.35" x2="13.85" y2="12.35" opacity={0.85} />
      <line x1="10.15" y1="13.55" x2="13.85" y2="13.55" opacity={0.85} />
      {/* Shock mount + stand */}
      <path d="M8.15 15.85v1.15q0 1.75 3.85 2.45Q15.85 18.75 15.85 17v-1.15" />
      <line x1="12" y1="19.45" x2="12" y2="20.85" />
      <path d="M7.25 20.85h9.5" />
    </svg>
  );
}
