import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

const invokeMock = vi.fn();

vi.mock("../../shared/ipc/athenaIpc", () => ({
  athenaInvoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
  SESSION_EXPIRED_EVENT: "athena:session-expired",
  setSessionActive: () => {},
}));

vi.mock("../../shared/ipc/athenaRuntime", () => ({ isAthenaDesktopRuntime: () => false }));

import { AuthProvider, useAuth } from "./AuthContext";

const TOKEN_KEY = "athena_lan_session_token";

function WhoAmI() {
  const { user } = useAuth();
  return <p>{user ? `signed in as ${user.name}` : "lock screen"}</p>;
}

describe("LAN session resume after a page reload", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    sessionStorage.clear();
  });
  afterEach(() => {
    cleanup();
    sessionStorage.clear();
  });

  it("restores the user when the tab still holds a live session token", async () => {
    sessionStorage.setItem(TOKEN_KEY, "tok-123");
    invokeMock.mockResolvedValue({ id: "p1", name: "test-operator", rank_level: 5, clearance_level: 5 });
    render(
      <AuthProvider>
        <WhoAmI />
      </AuthProvider>,
    );
    expect(screen.getByRole("status").textContent).toMatch(/signing you back in/i);
    expect(await screen.findByText("signed in as test-operator")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("refresh_auth_session", undefined);
  });

  it("falls back to the lock screen and drops the token when the server no longer knows it", async () => {
    sessionStorage.setItem(TOKEN_KEY, "tok-stale");
    invokeMock.mockRejectedValue(new Error("Not authenticated."));
    render(
      <AuthProvider>
        <WhoAmI />
      </AuthProvider>,
    );
    expect(await screen.findByText("lock screen")).toBeTruthy();
    expect(sessionStorage.getItem(TOKEN_KEY)).toBeNull();
  });

  it("does not call the server when there is no token (fresh tab)", () => {
    render(
      <AuthProvider>
        <WhoAmI />
      </AuthProvider>,
    );
    expect(screen.getByText("lock screen")).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
