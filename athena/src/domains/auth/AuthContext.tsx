import React, { createContext, useCallback, useContext, useMemo, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";

export type AuthUser = {
  id: string;
  name: string;
  division: string;
  rank_level: number;
  clearance_level?: number;
  display_name: string | null;
  operational_status: "active" | "on_mission" | "mia" | "kia";
  /** Directorate key, e.g. Infrastructure */
  directorate: string | null;
  tier: "sector_lead" | "personnel" | null;
  job_title: string | null;
  /** Designated ATHENA creator seat — not granted by Level V rank alone. */
  athena_architect?: boolean;
};

function coerceAuthUser(raw: unknown): AuthUser {
  const r = (raw ?? {}) as any;
  const clearance = Number.isFinite(r.clearance_level) ? Number(r.clearance_level) : Number(r.clearanceLevel ?? NaN);
  const rank = Number.isFinite(r.rank_level) ? Number(r.rank_level) : Number(r.rankLevel ?? 1);
  return {
    id: String(r.id ?? ""),
    name: String(r.name ?? ""),
    division: String(r.division ?? ""),
    rank_level: rank,
    clearance_level: Number.isFinite(clearance) ? clearance : rank,
    display_name: (r.display_name ?? r.displayName ?? null) as string | null,
    operational_status: (r.operational_status ?? r.operationalStatus ?? "active") as AuthUser["operational_status"],
    directorate: (r.directorate ?? null) as string | null,
    tier: (r.tier ?? null) as AuthUser["tier"],
    job_title: (r.job_title ?? r.jobTitle ?? null) as string | null,
    athena_architect: Boolean(r.athena_architect ?? r.athenaArchitect ?? false),
  };
}

export type AuthContextValue = {
  user: AuthUser | null;
  rankLevel: number | null;
  clearanceLevel: number | null;
  division: string | null;
  directorate: string | null;
  jobTitle: string | null;
  login: (name: string, pin: string) => Promise<void>;
  loginGuest: () => Promise<void>;
  logout: () => Promise<void>;
  setMyPersonnelDisplayName: (displayName: string | null) => Promise<void>;
  setMyLoginName: (newName: string) => Promise<void>;
  setMyPin: (currentPin: string, newPin: string) => Promise<void>;
  refreshSession: () => Promise<void>;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(null);

  const login = useCallback(async (name: string, pin: string) => {
    const profile = await invoke<unknown>("authenticate_pin", { name, pin });
    setUser(coerceAuthUser(profile));
  }, []);

  const loginGuest = useCallback(async () => {
    const profile = await invoke<unknown>("authenticate_guest");
    setUser(coerceAuthUser(profile));
  }, []);

  const logout = useCallback(async () => {
    await invoke("logout");
    setUser(null);
  }, []);

  const setMyPersonnelDisplayName = useCallback(async (displayName: string | null) => {
    const profile = await invoke<unknown>("set_my_personnel_display_name", {
      displayName,
    });
    setUser(coerceAuthUser(profile));
  }, []);

  const setMyLoginName = useCallback(async (newName: string) => {
    const profile = await invoke<unknown>("set_my_personnel_login_name", {
      newName: newName.trim(),
    });
    setUser(coerceAuthUser(profile));
  }, []);

  const setMyPin = useCallback(async (currentPin: string, newPin: string) => {
    await invoke("set_my_personnel_pin", {
      currentPin,
      newPin,
    });
  }, []);

  const refreshSession = useCallback(async () => {
    const profile = await invoke<unknown>("refresh_auth_session");
    setUser(coerceAuthUser(profile));
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      rankLevel: user?.rank_level ?? null,
      clearanceLevel: user?.clearance_level ?? user?.rank_level ?? null,
      division: user?.division ?? null,
      directorate: user?.directorate ?? null,
      jobTitle: user?.job_title ?? null,
      login,
      loginGuest,
      logout,
      setMyPersonnelDisplayName,
      setMyLoginName,
      setMyPin,
      refreshSession,
    }),
    [user, login, loginGuest, logout, setMyPersonnelDisplayName, setMyLoginName, setMyPin, refreshSession]
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used within AuthProvider");
  }
  return ctx;
}
