import React, { createContext, useCallback, useContext, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type AuthUser = {
  id: string;
  name: string;
  division: string;
  rank_level: number;
  display_name: string | null;
};

export type AuthContextValue = {
  user: AuthUser | null;
  rankLevel: number | null;
  division: string | null;
  login: (name: string, pin: string) => Promise<void>;
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
    const profile = await invoke<AuthUser>("authenticate_pin", { name, pin });
    setUser(profile);
  }, []);

  const logout = useCallback(async () => {
    await invoke("logout");
    setUser(null);
  }, []);

  const setMyPersonnelDisplayName = useCallback(async (displayName: string | null) => {
    const profile = await invoke<AuthUser>("set_my_personnel_display_name", {
      displayName,
    });
    setUser(profile);
  }, []);

  const setMyLoginName = useCallback(async (newName: string) => {
    const profile = await invoke<AuthUser>("set_my_personnel_login_name", {
      newName: newName.trim(),
    });
    setUser(profile);
  }, []);

  const setMyPin = useCallback(async (currentPin: string, newPin: string) => {
    await invoke("set_my_personnel_pin", {
      currentPin,
      newPin,
    });
  }, []);

  const refreshSession = useCallback(async () => {
    const profile = await invoke<AuthUser>("refresh_auth_session");
    setUser(profile);
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      rankLevel: user?.rank_level ?? null,
      division: user?.division ?? null,
      login,
      logout,
      setMyPersonnelDisplayName,
      setMyLoginName,
      setMyPin,
      refreshSession,
    }),
    [user, login, logout, setMyPersonnelDisplayName, setMyLoginName, setMyPin, refreshSession]
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
