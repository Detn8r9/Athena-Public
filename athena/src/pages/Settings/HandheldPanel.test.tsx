import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();

vi.mock("../../shared/ipc/athenaIpc", () => ({
  athenaInvoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

import { HandheldPanel } from "./HandheldPanel";

const characters = [
  { id: "wren", name: "Wren", blurb: "Pathfinder." },
  { id: "bastion", name: "Bastion", blurb: "Gate knight." },
];
const device = {
  deviceId: "a1b2c3d4e5f6",
  ip: "192.168.68.80",
  rssi: -60,
  firmware: "2.1",
  lastSeen: Math.floor(Date.now() / 1000),
  online: true,
};

describe("HandheldPanel", () => {
  beforeEach(() => invokeMock.mockReset());
  afterEach(cleanup);

  it("shows the chosen companion and the handhelds signed in as this operator", async () => {
    invokeMock.mockResolvedValue({ character: "wren", characters, devices: [device] });
    render(<HandheldPanel />);
    expect(await screen.findByText("Handheld D4:E5:F6")).toBeInTheDocument();
    expect(screen.getByText("Online")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Wren/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /Bastion/ })).toHaveAttribute("aria-checked", "false");
  });

  it("saves a new companion on the station", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      Promise.resolve({ character: cmd === "handheld_set_character" ? "bastion" : "wren", characters, devices: [] }),
    );
    render(<HandheldPanel />);
    fireEvent.click(await screen.findByRole("radio", { name: /Bastion/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("handheld_set_character", { character: "bastion" }));
    await waitFor(() => expect(screen.getByRole("radio", { name: /Bastion/ })).toHaveAttribute("aria-checked", "true"));
  });

  it("explains how to link a handheld when none has signed in", async () => {
    invokeMock.mockResolvedValue({ character: "wren", characters, devices: [] });
    render(<HandheldPanel />);
    expect(await screen.findByText(/No handheld has signed in as you yet/)).toBeInTheDocument();
  });
});
