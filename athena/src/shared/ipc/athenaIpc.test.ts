import { describe, expect, it, vi } from "vitest";
import { safeUnlisten, withCamelCaseArgs } from "./athenaIpc";

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

describe("withCamelCaseArgs", () => {
  it("adds a camelCase copy of snake_case keys and keeps the original", () => {
    expect(withCamelCaseArgs({ item_name: "Rice", quantity: 3 })).toEqual({
      item_name: "Rice",
      itemName: "Rice",
      quantity: 3,
    });
  });
  it("does not overwrite a camelCase key that is already there", () => {
    expect(withCamelCaseArgs({ item_id: 1, itemId: 2 })).toEqual({ item_id: 1, itemId: 2 });
  });
  it("leaves nested objects and undefined args alone", () => {
    const plan = { interval_hours: 1 };
    expect(withCamelCaseArgs({ plan })).toEqual({ plan });
    expect(withCamelCaseArgs(undefined)).toBeUndefined();
  });
});
