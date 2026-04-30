import type { ComponentProps } from "react";
import { RadioTower } from "lucide-react";

export type CommsHubIconProps = ComponentProps<typeof RadioTower>;

/**
 * Communications Hub — Lucide `RadioTower` (broadcast arcs + mast) so sizing and stroke
 * match other dashboard / header icons (e.g. MessageSquare, Map).
 */
export function CommsHubIcon({ size = 24, strokeWidth = 2, className, ...props }: CommsHubIconProps) {
  return (
    <RadioTower size={size} strokeWidth={strokeWidth} className={className} aria-hidden {...props} />
  );
}
