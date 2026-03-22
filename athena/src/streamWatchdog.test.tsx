import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import { useEffect, useRef, useState } from "react";
import { evaluateMainChatWatchdog } from "./streamWatchdog";

/** Must match the fake clock base used in tests */
const WATCHDOG_TEST_T0 = 10_000_000;

/**
 * Minimal harness that mirrors the main-chat watchdog interval in App.tsx:
 * 1s tick, uses the same `evaluateMainChatWatchdog` helper.
 */
function MainChatWatchdogHarness() {
  const [sending, setSending] = useState(true);
  const [stallMessage, setStallMessage] = useState<string | null>(null);
  const streamSendStartRef = useRef(WATCHDOG_TEST_T0);
  const streamLastChunkAtRef = useRef<number | null>(WATCHDOG_TEST_T0);
  const streamGotFirstChunkRef = useRef(true);

  useEffect(() => {
    if (!sending) return;
    const tick = () => {
      const now = Date.now();
      const w = evaluateMainChatWatchdog({
        nowMs: now,
        sending: true,
        sendStartMs: streamSendStartRef.current,
        lastChunkAtMs: streamLastChunkAtRef.current,
        gotFirstChunk: streamGotFirstChunkRef.current,
      });
      if (w.action === "stall_first_timeout" || w.action === "stall_idle") {
        setStallMessage(w.message);
        setSending(false);
      }
    };
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [sending]);

  return (
    <div>
      {stallMessage ? (
        <div className="chat-stream-stall-banner" role="alert">
          {stallMessage}
        </div>
      ) : null}
      <span data-testid="sending-state">{sending ? "sending" : "idle"}</span>
    </div>
  );
}

describe("main chat stream watchdog (Phase 1)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("after 30s idle with no new chunks, clears sending and shows stall banner", async () => {
    const t0 = WATCHDOG_TEST_T0;
    vi.setSystemTime(t0);

    render(<MainChatWatchdogHarness />);

    expect(screen.getByTestId("sending-state")).toHaveTextContent("sending");

    // Last chunk at t0; first chunk already received. Jump forward 31s — exceeds 30s idle.
    vi.setSystemTime(t0 + 31_000);

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });

    expect(screen.getByTestId("sending-state")).toHaveTextContent("idle");
    const banner = screen.getByRole("alert");
    expect(banner).toHaveClass("chat-stream-stall-banner");
    expect(banner).toHaveTextContent("The local AI stalled (no output for 30s).");
  });
});
