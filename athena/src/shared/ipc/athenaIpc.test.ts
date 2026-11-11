import { describe, expect, it, vi } from "vitest";
import { safeUnlisten } from "./athenaIpc";

describe("safeUnlisten", () => {
  it("calls the underlying unlisten once however many times it is invoked", () => {
    const inner = vi.fn();
    const un = safeUnlisten(inner);
    un();
    un();
    un();
    expect(inner).toHaveBeenCalledTimes(1);
  });

  it("swallows a synchronous throw from an already-removed listener", () => {
    const un = safeUnlisten(() => {
      throw new TypeError("undefined is not an object (evaluating 'listeners[eventId].handlerId')");
    });
    expect(() => un()).not.toThrow();
  });

  it("swallows an async rejection so it never becomes an unhandled rejection", async () => {
    const rejection = Promise.reject(new Error("already unlistened"));
    const un = safeUnlisten((() => rejection) as unknown as () => void);
    expect(() => un()).not.toThrow();
    await expect(rejection.catch(() => "handled")).resolves.toBe("handled");
  });
});
