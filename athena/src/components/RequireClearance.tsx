import React from "react";
import { Lock } from "lucide-react";
import { useAuth } from "../auth/AuthContext";

export function RequireClearance({
  required_level,
  children,
  fallback = null,
}: {
  required_level: number;
  children: React.ReactNode;
  fallback?: React.ReactNode;
}) {
  const { user } = useAuth();
  const lvl = user?.clearance_level ?? user?.rank_level ?? 0;
  if (lvl >= required_level) return <>{children}</>;
  if (fallback !== null) return <>{fallback}</>;
  return (
    <span className="clearance-locked" title={`Locked (requires clearance ${required_level}+)`} aria-label="Locked">
      <Lock size={16} strokeWidth={2} aria-hidden />
    </span>
  );
}

